import os
import sys
import glob
import time
import hashlib
import textwrap
import requests
import httpx
import socket
import json
import random
import traceback
from fastapi.responses import StreamingResponse
from fastapi import FastAPI, Query, HTTPException, Request, Body
from langchain_chroma import Chroma
from langchain_ollama import OllamaEmbeddings
from langchain_community.document_loaders import DirectoryLoader, TextLoader
from langchain_text_splitters import RecursiveCharacterTextSplitter
from ddgs import DDGS
from unstructured.cleaners.core import clean_extra_whitespace, clean_non_ascii_chars, replace_unicode_quotes
from datetime import datetime, timezone, timedelta
from pydantic import BaseModel, Field, PrivateAttr, ValidationError
from typing import Optional, List, Dict, Any, Union, Literal
from collections import Counter


# --- Configuration via variables d'environnement ---

PERSIST_DIR = os.environ.get("CHROMA_PERSIST_DIR", "/chroma_db")
CACHE_DIR = os.environ.get("RESPONSE_CACHE_DIR", "/response_cache")
GENERATE_MODEL = os.environ.get("GENERATE_MODEL", "deepseek-coder:6.7b-instruct")
CHAT_MODEL = os.environ.get("CHAT_MODEL", GENERATE_MODEL)
EMBED_MODEL = os.environ.get("EMBED_MODEL", "nomic-embed-text:latest")
SRC_PATH=os.environ.get("SRC_PATH", ".")
OLLAMA_HOST = os.environ.get("OLLAMA_HOST", "http://host.docker.internal:11434")
PROG_LANG = os.environ.get("PROG_LANG", "go")

try:
    CHUNCK_SIZE = int(os.environ.get("CHUNCK_SIZE", "300"))
except (ValueError, TypeError):
    CHUNCK_SIZE = 300

try:
    CHUNCK_OVERLAP = int(os.environ.get("CHUNCK_OVERLAP", "50"))
except (ValueError, TypeError):
    CHUNCK_OVERLAP = 50

try:
    QUERY_TIMEOUT = int(os.environ.get("QUERY_TIMEOUT", "120"))
except (ValueError, TypeError):
    QUERY_TIMEOUT = 120

DDGS_SEARCH_ENABLED = True


os.makedirs(PERSIST_DIR, exist_ok=True)
os.makedirs(CACHE_DIR, exist_ok=True)


# --- Nettoyage du code ---
def clean_code_content(content: str) -> str:
    cleaned = replace_unicode_quotes(content)
    cleaned = clean_non_ascii_chars(cleaned)
    cleaned = clean_extra_whitespace(cleaned)
    return cleaned

# --- Cache simple ---
def get_cache_key(question: str) -> str:
    return hashlib.md5(question.encode()).hexdigest()

# --- Hot-reload : hash du code ---
def hash_code_dir(paths: list) -> str:
    m = hashlib.md5()
    for path in paths:
        abs_path = os.path.join("/code", path) if path != "." else "/code"
        for f in glob.glob(f"{abs_path}/**/*.go", recursive=True):
            try:
                with open(f, "rb") as file:
                    m.update(file.read())
            except Exception:
                continue
    return m.hexdigest()

# --- Wrapper Nomic Embeddings ---
from typing import List

class NomicEmbeddingsWrapper(OllamaEmbeddings):
    """Wrapper Ollama pour les embeddings de code, compatible Chroma."""
    _cached_dim: int = PrivateAttr()  # attribut interne non validé par Pydantic

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        # Calcul de la dimension une seule fois
        self._cached_dim = 768 #len(super().embed_query("Hello"))

    def _prefix_text(self, text: str, is_document: bool) -> str:
        """Ajoute un préfixe pour distinguer document vs query."""
        prefix = "search_document: " if is_document else "search_query: "
        return prefix + text

    def embed_documents(self, texts: List[str]) -> List[List[float]]:
        """Embeds documents en ajoutant le préfixe, retourne liste de vecteurs float."""
        prefixed_texts = [self._prefix_text(t, is_document=True) for t in texts]
        embeddings = super().embed_documents(prefixed_texts)
        # Normaliser les embeddings vides pour éviter les erreurs Chroma
        return [e if e else [0.0] * self.model_dimensions for e in embeddings]

    def embed_query(self, text: str) -> List[float]:
        """Embeds une query en ajoutant le préfixe."""
        emb = super().embed_query(self._prefix_text(text, is_document=False))
        # Normaliser embedding vide
        return emb if emb else [0.0] * self.model_dimensions

    @property
    def model_dimensions(self) -> int:
        return self._cached_dim

# --- FastAPI ---
app = FastAPI()

# --- Traitement des chemins ---


src_paths_directories = SRC_PATH.split(":")
if not src_paths_directories:
    src_paths_directories = ["."]

# --- Initialisation ---
vectorstore = None
code_hash = ""

def build_vectorstore():
    global vectorstore, code_hash, src_paths_directories
    print("🔹 Construction du vectorstore ...", file=sys.stderr)
    

    # Hash du code pour hot-reload
    new_hash = hash_code_dir(src_paths_directories)
    if vectorstore and new_hash == code_hash:
        print("🔹 Pas de changement dans /code, utilisation du vectorstore existant", file=sys.stderr)
        return
    code_hash = new_hash

    print(f"  🔹 Programmation language: {PROG_LANG}", file=sys.stderr)
    print(f"  🔹 Chunck size: {CHUNCK_SIZE}", file=sys.stderr)
    print(f"  🔹 Chunck overlap: {CHUNCK_OVERLAP}", file=sys.stderr)
    # Text splitter optimisé Go
    code_splitter = RecursiveCharacterTextSplitter.from_language(
        language=PROG_LANG,
        chunk_size=CHUNCK_SIZE,
        chunk_overlap=CHUNCK_OVERLAP, 
        keep_separator=True 
    )

    all_docs = []
    for path in src_paths_directories:
        abs_path = os.path.join("/code", path) if path != "." else "/code"
        print(f"   🔹 Chargement du code Go depuis: {abs_path}", file=sys.stderr)
        loader = DirectoryLoader(
            abs_path,
            glob="**/*.go",
            loader_cls=TextLoader,
            use_multithreading=True,
            loader_kwargs={'autodetect_encoding': True}
        )
        loaded_docs = loader.load()
        print(f"   🔸 {len(loaded_docs)} fichiers chargés", file=sys.stderr)
        for doc in loaded_docs:
            doc.page_content = clean_code_content(doc.page_content)
        all_docs.extend(loaded_docs)

    print(f"🔹 {len(all_docs)} documents après chargement", file=sys.stderr)

    embedding = NomicEmbeddingsWrapper(model=EMBED_MODEL, base_url=OLLAMA_HOST)

    chat_collection = Chroma.from_documents(
        documents=all_docs,
        embedding=embedding,
        persist_directory=PERSIST_DIR,
        collection_metadata={"hnsw:space": "cosine"},
        collection_name="chat_context"
    )

    splits = code_splitter.split_documents(all_docs)
    print(f"🔹 {len(splits)} chunks créés", file=sys.stderr)

    splits = [doc for doc in splits if doc.page_content.strip()]

    # Ajout des statistiques de longueur du split
    bins=50
    counter = Counter(int(len(split.page_content) / bins) * bins for split in splits)
    print("🔹 Histogramme de la longueur des splits :", file=sys.stderr)
    max_value = max(counter.values())
    for length, count in sorted(counter.items()):
        normalized_count = int((count / max_value) * 50)
        print(f"   {length+1:6}-{length+bins:-6}: { '#' * normalized_count}", file=sys.stderr)

    print(f"🔹 {len(splits)} fragments non vides à intégrer", file=sys.stderr)
    # Créer ou recharger Chroma

    # 2. Vectorstore pour la Génération (splits courts)
    gen_collection = Chroma.from_documents(
        documents=splits,  # Morceaux de 200-400 tokens
        embedding=embedding,
        collection_name="code_completion",
        persist_directory=PERSIST_DIR,
        collection_metadata={"hnsw:space": "cosine"}
    )

    vectorstore = {
        "chat": chat_collection,
        "generate": gen_collection
       }

    print("🔹 Vectorstore créé", file=sys.stderr)

# --- Formatage du contexte ---
def format_context(docs: list) -> str:
    context = []
            
    extraits = {}
    print("🔹 Les fichiers suivants ont été selectionnés:", file=sys.stderr)

    for i, doc in enumerate(docs):
        source = doc.metadata.get('source', 'unknown')
        filename = os.path.basename(source)
        print(f"  🔹 {filename} -- extrait {i+1} --", file=sys.stderr)
        if doc.page_content not in extraits:
            extraits[doc.page_content] = True
            context.append(f"### Fichier: {filename} (Extrait {i+1}) ###")
            context.append(textwrap.indent(doc.page_content, '    '))
            print(f"  🔹 {filename} -- fin extrait {i+1} --", file=sys.stderr)
        else:
            print(f"  🔸 {filename} -- extrait {i+1} duppliqué et éliminé --", file=sys.stderr)

    return "\n\n".join(context)

def format_iso_time_with_ns():
    # 1. Obtenir le timestamp actuel avec nanosecondes
    current_time_ns = time.time_ns()
    
    # 2. Convertir en datetime avec timezone locale
    dt = datetime.fromtimestamp(current_time_ns / 1e9).astimezone()
    
    # 3. Formater avec les nanosecondes et décalage horaire
    # - Extraire les nanosecondes
    nanoseconds = current_time_ns % 10**9
    
    # - Formater la partie datetime de base
    base_format = dt.strftime("%Y-%m-%dT%H:%M:%S")
    
    # - Ajouter les nanosecondes (9 chiffres)
    nano_format = f".{nanoseconds:09d}"
    
    # - Formater le décalage horaire
    utc_offset = dt.utcoffset()
    offset_hours = utc_offset.total_seconds() // 3600
    offset_minutes = (utc_offset.total_seconds() % 3600) // 60
    offset_sign = '-' if offset_hours < 0 else '+'
    offset_format = f"{offset_sign}{abs(int(offset_hours)):02d}:{int(offset_minutes):02d}"
    
    return base_format + nano_format + offset_format


# Modèles Pydantic pour l'API compatible Ollama
class GenerateRequest(BaseModel):
    model: str
    prompt: str
    system: Optional[str] = None
    template: Optional[str] = None
    stream: Optional[bool] = False
    options: Optional[Dict[str, Any]] = None

class ChatMessage(BaseModel):
    role: str
    content: str
    images: Optional[List[str]] = None

class ChatRequest(BaseModel):
    model: str
    messages: List[ChatMessage]
    format: Optional[str] = None
    options: Optional[Dict[str, Any]] = None
    stream: bool = True
    keep_alive: Optional[Union[str, int]] = None  # Modification ici

class EmbeddingRequest(BaseModel):
    model: str
    prompt: str
    options: Optional[Dict[str, Any]] = None

class EmbeddingResponse(BaseModel):
    embedding: List[float]

# Fonctions utilitaires
async def perform_rag_search(mode: Literal["generate", "chat"], prompt: str, k: int = 4) -> str:
    """Effectue une recherche RAG et retourne le contexte"""
    build_vectorstore()
    
    rag_docs = vectorstore[mode].similarity_search(prompt, k=k)
    return format_context(rag_docs) if rag_docs else "Aucun contexte trouvé."

async def perform_web_search(prompt: str, k: int = 2) -> str:
    """Effectue une recherche web et retourne les résultats"""
    if not DDGS_SEARCH_ENABLED:
        return "Recherche web désactivée"
    
    try:
        from ddgs import DDGS
        with DDGS() as ddgs:
            results = list(ddgs.text(prompt, max_results=k))
            print(f"🔹 {len(results)} résultats trouvés sur le web", file=sys.stderr)
            for i, r in enumerate(results):  
                print(f" - {i+1}. {r['title']} : {r['href']}", file=sys.stderr)
            
            web_info = "\n".join(
                f"- [{r['title']}]({r['href']}): {r['body'][:150]}..." 
                for r in results
            ) if results else "Aucun résultat web trouvé."
    except Exception as e:
        return f"Erreur recherche web: {str(e)}"

def build_enhanced_prompt(
                 mode: Literal["generate", "chat"], 
                 question: str,
                 rag_context: str,
                 web_context: str):
    
    if mode == "chat":
        prompt = f"""
# Consigne

Vous êtes un expert en programmation {PROG_LANG}. Répondez à la question en utilisant le contexte fourni (extraits de code) et les informations web si disponibles.
Autant que possible tu indiqueras tes sources, url, nom du fichier source...

# Contexte de la question:

## **Contexte Code (extraits pertinents):**

{rag_context}

## **Informations Web:**

{web_context}

## **Instructions:**
- Répondez de manière concise et précise à la question
- Si la réponse se trouve dans le contexte code, citez le fichier et l'extrait correspondant
- Si vous utilisez les informations web, citez la source
- Si la question est en anglais, répondez en anglais. Sinon, en français
- Pour les extraits de code, conservez le formatage et l'indentation

# **Question** à laquelle tu dois répondre

{question}
"""
    else:
        prompt=f"""
# Consigne

Vous êtes un expert en programmation {PROG_LANG}. Essayer de concevoir un petit bout de code permetant de résoudre la question

# Contexte de la question:

## **Contexte Code (extraits pertinents):**

{rag_context}

## **Instructions:**

- Rédigez les commentaires de code dans la même langue que le code qui vous est fourni. À défaut en anglais.
- Nommez les variables dans la même langue que le code qui vous est fourni. À défaut en anglais.
- Si la question est en anglais, répondez en anglais. Sinon, en français
- Pour les extraits de code, conservez le formatage et l'indentation

# **Question** à laquelle tu dois répondre

{question}
"""

    return prompt

# --- Fonctions utilitaires factorisées ---
async def build_augmented_prompt(
                 mode: Literal["generate", "chat"], 
                 question: str
        ) -> str:
    """Construit un prompt enrichi avec contextes RAG et web"""

    rag_context = await perform_rag_search(mode,question,k= 2 if mode=='chat' else 8)

    if mode == "chat":
        web_context = await perform_web_search(question)
    else:
        web_context = ""

    return build_enhanced_prompt(
        mode=mode,
        question=question,
        rag_context=rag_context,
        web_context=web_context
    )

async def _stream_ollama_response(response: httpx.Response, model_name: str):
    """Générateur pour le streaming de la réponse de chat au format Ollama"""
    start_time = datetime.now(timezone.utc).isoformat()
    async for line in response.aiter_lines():
        if line.strip():
            try:
                chunk = json.loads(line)
                
                # Construction du message conforme à l'API Ollama
                message_chunk = {
                    "model": model_name,
                    "created_at": start_time,
                    "message": {
                        "role": "assistant",
                        "content": chunk.get("message", {}).get("content", "") if "message" in chunk else chunk.get("content", "")
                    },
                    "done": chunk.get("done", False)
                }
                
                # Ajout des champs optionnels
                for field in ["total_duration", "load_duration", "prompt_eval_count", "eval_count"]:
                    if field in chunk:
                        message_chunk[field] = chunk[field]
                
                yield f"data: {json.dumps(message_chunk)}\n\n"
                
            except json.JSONDecodeError:
                yield "event: error\ndata: Invalid JSON chunk\n\n"
    
    yield "event: end\ndata: Stream completed\n\n"


# --- Endpoints API ---
@app.post("/api/generate")
async def generate_endpoint(request_data: GenerateRequest = Body(...)):
    try:
        # Construction du prompt enrichi
        enhanced_prompt = await build_augmented_prompt("generate",request_data.prompt)
        
        # Appel à Ollama
        ollama_payload = {
            "model": GENERATE_MODEL,
            "prompt": enhanced_prompt,
            "stream": request_data.stream,
            "options": request_data.options or {}
        }
        
        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{OLLAMA_HOST}/api/generate",
                json=ollama_payload,
                timeout=QUERY_TIMEOUT
            )
            response.raise_for_status()
            
            if not request_data.stream:
                return response.json()
            else:
                # --- CORRECTION DU STREAMING ---
                async def generate_stream():
                    """Générateur pour le streaming des résultats"""
                    async for chunk in response.aiter_text():
                        # Transférer directement les chunks
                        yield chunk
                        
                    # Fermeture propre du stream
                    # yield "data: [DONE]\n\n"
                
                # Utilisez text/plain au lieu de text/event-stream
                return StreamingResponse(
                    generate_stream(),
                    media_type="text/plain",
                    headers={
                        "Cache-Control": "no-cache",
                        "Connection": "keep-alive",
                        "X-Accel-Buffering": "no"
                    }
                )
                        
    except httpx.RequestError as e:
        raise HTTPException(500, f"Erreur de connexion à Ollama: {str(e)}")
    except Exception as e:
        raise HTTPException(500, f"Erreur interne: {str(e)}")

@app.post("/api/chat")
async def chat_endpoint(request_data: ChatRequest):
    try:
        messages = [msg.dict() for msg in request_data.messages]
        
        if messages and messages[-1]["role"] == "user":
            original_question = messages[-1]["content"]
            try:
                # Limiter la taille du contexte
                augmented_prompt = await build_augmented_prompt("chat",original_question)
                messages[-1]["content"] = augmented_prompt[-8000:]  # Truncate to the last 8000 chars
                print(f"🔹 Prompt enrichi ({len(augmented_prompt)} caractères)", file=sys.stderr)
            except Exception as e:
                print(f"⚠️ Erreur d'enrichissement: {str(e)}", file=sys.stderr)
                messages[-1]["content"] = original_question  # Fallback to original
        
        # Préparation du payload pour Ollama
        ollama_payload = {
            "model": CHAT_MODEL,
            "messages": messages,
            "stream": request_data.stream,
            "options": request_data.options or {}
        }

        if request_data.keep_alive is not None:
            if isinstance(request_data.keep_alive, int):
                ollama_payload["keep_alive"] = f"{request_data.keep_alive}s"
            else:
                ollama_payload["keep_alive"] = request_data.keep_alive
        
        sopload = json.dumps(ollama_payload, indent=2)
        print(f" 🔹 Taille du Payload vers Ollama : {len(sopload)} octets...", file=sys.stderr)
        print(f" 🔹 Début du payload : {sopload}...", file=sys.stderr)

        # Appel à Ollama
        async with httpx.AsyncClient() as client:
            try:
                response = await client.post(
                    f"{OLLAMA_HOST}/api/chat",
                    json=ollama_payload,
                    timeout=QUERY_TIMEOUT
                )
                response.raise_for_status()
                
                # Gestion des réponses NON-STREAMING
                if not request_data.stream:
                    return response.json()
                
                # --- CORRECTION DU STREAMING ---
                async def generate_stream():
                    """Générateur pour le streaming des résultats"""
                    async for chunk in response.aiter_text():
                        # Transférer directement les chunks
                        yield chunk
                        
                    # Fermeture propre du stream
                    # yield "data: [DONE]\n\n"
                
                # Utilisez text/plain au lieu de text/event-stream
                return StreamingResponse(
                    generate_stream(),
                    media_type="text/plain",
                    headers={
                        "Cache-Control": "no-cache",
                        "Connection": "keep-alive",
                        "X-Accel-Buffering": "no"
                    }
                )
    # ... [gestion des erreurs existante] ...
            except httpx.HTTPStatusError as e:
                error_detail = e.response.text if e.response else str(e)
                print(f"🚨 Erreur HTTP Ollama ({e.response.status_code}): {error_detail}", file=sys.stderr)
                raise HTTPException(502, f"Erreur Ollama: {error_detail}")
            except httpx.RequestError as e:
                print(f"🚨 Erreur réseau Ollama: {str(e)}", file=sys.stderr)
                raise HTTPException(503, f"Ollama non disponible: {str(e)}")
                
    except Exception as e:
        print(f"🚨 Erreur interne: {traceback.format_exc()}", file=sys.stderr)
        raise HTTPException(500, f"Erreur interne: {str(e)}")

@app.post("/api/embeddings")
async def embeddings(request: EmbeddingRequest):
    """Proxy direct pour les embeddings"""
    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(
                f"{OLLAMA_HOST}/api/embeddings",
                json=request.dict()
            )
            response.raise_for_status()
            return response.json()
            
        except httpx.RequestError as e:
            raise HTTPException(500, f"Erreur de connexion à Ollama: {str(e)}")

@app.get("/api/tags")
async def list_models():
    """Proxy pour lister les modèles disponibles"""
    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(f"{OLLAMA_HOST}/api/tags")
            response.raise_for_status()
            return response.json()
            
        except httpx.RequestError as e:
            raise HTTPException(500, f"Erreur de connexion à Ollama: {str(e)}")

# Initialisation du vectorstore (à adapter à votre code)
@app.on_event("startup")
async def startup_event():
    global vectorstore
    build_vectorstore()
    print("🔹 Initialisation du serveur proxy Ollama+RAG")

# Endpoint supplémentaire pour le contrôle
@app.get("/control/enable_web_search")
async def enable_web_search(enabled: bool = True):
    global DDGS_SEARCH_ENABLED
    DDGS_SEARCH_ENABLED = enabled
    return {"status": "success", "web_search_enabled": enabled}

# Endpoint de debug simplifié
@app.post("/debug")
async def debug_endpoint(request: Request):
    """Endpoint de débogage simplifié"""
    try:
        body = await request.json()
        return {
            "status": "success",
            "received_body": body
        }
    except json.JSONDecodeError:
        raise HTTPException(400, "Invalid JSON format")
