import os
import sys
import glob
import time
import hashlib
import textwrap
import requests
import httpx
import socket
from fastapi.responses import StreamingResponse
from fastapi import FastAPI, Query, HTTPException, Request, Body
from langchain_community.vectorstores import Chroma
from langchain_ollama import OllamaEmbeddings
from langchain_community.document_loaders import DirectoryLoader, TextLoader
from langchain_text_splitters import RecursiveCharacterTextSplitter
from ddgs import DDGS
from unstructured.cleaners.core import clean_extra_whitespace, clean_non_ascii_chars, replace_unicode_quotes
from datetime import datetime, timezone, timedelta
from pydantic import BaseModel, Field, PrivateAttr, ValidationError
from typing import Optional, List, Dict, Any


# --- Configuration via variables d'environnement ---
PERSIST_DIR = os.environ.get("CHROMA_PERSIST_DIR", "/chroma_db")
CACHE_DIR = os.environ.get("RESPONSE_CACHE_DIR", "/response_cache")
MODEL_NAME = os.environ.get("OLLAMA_MODEL", "llama3:13b")
EMBED_MODEL = os.environ.get("EMBED_MODEL", "mxbai-embed-large:latest")
SRC_PATH=os.environ.get("SRC_PATH", ".")

# Configuration
OLLAMA_BASE_URL = "http://127.0.0.1:11434"
VECTORSTORE = None  # Initialisé ailleurs
DDGS_SEARCH_ENABLED = True


os.makedirs(PERSIST_DIR, exist_ok=True)
os.makedirs(CACHE_DIR, exist_ok=True)

# Configuration de l'URL de base d'Ollama
def get_ollama_base_url():
    """Détermine dynamiquement l'URL d'Ollama"""
    # 1. Vérifier la variable d'environnement
    if "OLLAMA_BASE_URL" in os.environ:
        return os.environ["OLLAMA_BASE_URL"]
    
    # 2. Tester la connectivité locale
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.settimeout(1)
            s.connect(("localhost", 11434))
        return "http://localhost:11434"
    except (socket.timeout, ConnectionRefusedError):
        pass
    
    # 3. Essayer l'adresse spéciale Docker
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.settimeout(1)
            s.connect(("host.docker.internal", 11434))
        return "http://host.docker.internal:11434"
    except (socket.timeout, ConnectionRefusedError):
        pass
    # 4. Fallback pour les environnements cloud
    return "http://127.0.0.1:11434"


OLLAMA_BASE_URL = get_ollama_base_url()


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


paths = SRC_PATH.split(":")
if not paths:
    paths = ["."]

# --- Initialisation ---
vectorstore = None
code_hash = ""

def build_vectorstore():
    global vectorstore, code_hash, paths
    print("🔹 Construction du vectorstore...", file=sys.stderr)

    # Hash du code pour hot-reload
    new_hash = hash_code_dir(paths)
    if vectorstore and new_hash == code_hash:
        print("🔹 Pas de changement dans /code, utilisation du vectorstore existant", file=sys.stderr)
        return
    code_hash = new_hash

    # Text splitter optimisé Go
    go_splitter = RecursiveCharacterTextSplitter.from_language(
        language="go",
        chunk_size=800,
        chunk_overlap=150 #,
        #separators=["\n\n", "\nfunc ", "}\n\n", "\n//", "\n/*", "\t"]
    )

    all_docs = []
    for path in paths:
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
    splits = go_splitter.split_documents(all_docs)
    print(f"🔹 {len(splits)} chunks créés", file=sys.stderr)

    embedding = NomicEmbeddingsWrapper(model=EMBED_MODEL, base_url=OLLAMA_BASE_URL)

    splits = [doc for doc in splits if doc.page_content.strip()]

    print(f"🔹 {len(splits)} fragments non vides à intégrer", file=sys.stderr)
    # Créer ou recharger Chroma
    vectorstore = Chroma.from_documents(
        documents=splits,
        embedding=embedding,
        persist_directory=PERSIST_DIR,
        collection_metadata={"hnsw:space": "cosine"}
    )

    print("🔹 Vectorstore créé", file=sys.stderr)

# --- Formatage du contexte ---
def format_context(docs: list) -> str:
    context = []
    for i, doc in enumerate(docs):
        source = doc.metadata.get('source', 'unknown')
        filename = os.path.basename(source)
        context.append(f"### Fichier: {filename} (Extrait {i+1}) ###")
        context.append(textwrap.indent(doc.page_content, '    '))
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
    stream: bool = False
    keep_alive: Optional[str] = None

class EmbeddingRequest(BaseModel):
    model: str
    prompt: str
    options: Optional[Dict[str, Any]] = None

class EmbeddingResponse(BaseModel):
    embedding: List[float]

# Fonctions utilitaires
async def perform_rag_search(prompt: str, k: int = 4) -> str:
    """Effectue une recherche RAG et retourne le contexte"""
    build_vectorstore()
    
    rag_docs = vectorstore.similarity_search(prompt, k=k)
    return format_context(rag_docs) if rag_docs else "Aucun contexte trouvé."

async def perform_web_search(prompt: str, k: int = 2) -> str:
    """Effectue une recherche web et retourne les résultats"""
    if not DDGS_SEARCH_ENABLED:
        return "Recherche web désactivée"
    
    try:
        from duckduckgo_search import DDGS
        with DDGS() as ddgs:
            results = list(ddgs.text(prompt, max_results=k))
            web_info = "\n".join(f"- [{r['title']}]({r['href']}): {r['body'][:150]}..." for r in results) if results else "Aucun résultat web trouvé."
    except Exception as e:
        return f"Erreur recherche web: {str(e)}"

def build_enhanced_prompt(original_prompt: str, rag_context: str, web_context: str) -> str:
    """Construit un prompt enrichi avec les contextes"""
    return f"""
### CONTEXTE RAG (Code) ###
{rag_context or "Aucun contexte code disponible"}

### CONTEXTE WEB ###
{web_context or "Aucune information web disponible"}

### QUESTION UTILISATEUR ###
{original_prompt}
"""


# Endpoints compatibles Ollama
@app.post("/api/generate")
async def generate_endpoint(request_data: GenerateRequest = Body(...)):
    """Endpoint pour la génération avec gestion du streaming"""
    try:
        # Récupération des contextes RAG et web
        rag_context = await perform_rag_search(request_data.prompt)
        web_context = await perform_web_search(request_data.prompt)
        
        # Construction du prompt enrichi
        enhanced_prompt = build_enhanced_prompt(
            original_prompt=request_data.prompt,
            rag_context=rag_context,
            web_context=web_context
        )
        
        # Préparation du payload pour Ollama
        ollama_payload = {
            "model": request_data.model,
            "prompt": enhanced_prompt,
            "stream": request_data.stream,
            "options": request_data.options or {}
        }
        
        # Appel à Ollama
        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{OLLAMA_BASE_URL}/api/generate",
                json=ollama_payload,
                timeout=120.0
            )
            response.raise_for_status()
            
            # Gestion des réponses NON-STREAMING
            if not request_data.stream:
                result = response.json()
                return {
                    "model": result["model"],
                    "response": result["response"],
                    "done": result["done"],
                    "context": result.get("context"),
                    "total_duration": result.get("total_duration")
                }
            
            # Gestion des réponses STREAMING
            else:
                async def generate():
                    """Générateur pour le streaming des résultats"""
                    full_response = ""
                    async for line in response.aiter_lines():
                        if line.strip():
                            try:
                                chunk = json.loads(line)
                                
                                # 1. Format SSE valide avec double newline
                                yield f"data: {json.dumps(chunk)}\n\n"
                                
                                # 2. Accumuler la réponse complète pour les logs
                                full_response += chunk.get("response", "")
                                
                                # 3. Envoyer périodiquement un keep-alive
                                if random.random() < 0.1:  # 10% des chunks
                                    yield ": keep-alive\n\n"
                                
                                # 4. Fin du stream
                                if chunk.get("done", False):
                                    break
                            except json.JSONDecodeError:
                                print(f"⚠️ Ligne JSON invalide: {line}")
                                yield f"event: error\ndata: Invalid JSON line\n\n"
                    
                    # 5. Envoyer un message de fin explicite
                    yield "event: end\ndata: Stream completed\n\n"
                    
                    # 6. Log de la réponse complète
                    print(f"🔹 Réponse complète ({len(full_response)} caractères): {full_response[:200]}...", file=sys.stderr)
                    
                    # Log de la réponse complète (optionnel)
                    print(f"🔹 Réponse complète: {full_response}")
                
                # 7. Configuration de la réponse avec des headers spécifiques
                return StreamingResponse(
                    generate(),
                    media_type="text/event-stream",
                    headers={
                        "Cache-Control": "no-cache",
                        "Connection": "keep-alive",
                        "X-Accel-Buffering": "no"  # Important pour Nginx
                    }
                )    
                        
    except httpx.RequestError as e:
        raise HTTPException(500, f"Erreur de connexion à Ollama: {str(e)}")
    except json.JSONDecodeError as e:
        raise HTTPException(500, f"Erreur de décodage JSON: {str(e)}")
    except Exception as e:
        raise HTTPException(500, f"Erreur interne: {str(e)}")


@app.post("/api/chat")
async def chat(request: ChatRequest):
    """Endpoint /api/chat avec enrichissement du dernier message"""
    # Copie profonde des messages
    processed_messages = [msg.dict() for msg in request.messages]
    
    # Enrichissement uniquement du dernier message utilisateur
    if processed_messages and processed_messages[-1]["role"] == "user":
        last_msg = processed_messages[-1]["content"]
        
        rag_context = await perform_rag_search(last_msg)
        web_context = await perform_web_search(last_msg)
        
        enhanced_content = build_enhanced_prompt(
            original_prompt=last_msg,
            rag_context=rag_context,
            web_context=web_context
        )
        
        processed_messages[-1]["content"] = enhanced_content
    
    # Appel au vrai Ollama
    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(
                f"{OLLAMA_BASE_URL}/api/chat",
                json={
                    "model": request.model,
                    "messages": processed_messages,
                    "format": request.format,
                    "options": request.options,
                    "stream": request.stream,
                    "keep_alive": request.keep_alive
                },
                timeout=120.0
            )
            response.raise_for_status()
            
            if request.stream:
                return response.iter_lines()
            
            return response.json()
            
        except httpx.RequestError as e:
            raise HTTPException(500, f"Erreur de connexion à Ollama: {str(e)}")

@app.post("/api/embeddings")
async def embeddings(request: EmbeddingRequest):
    """Proxy direct pour les embeddings"""
    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(
                f"{OLLAMA_BASE_URL}/api/embeddings",
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
            response = await client.get(f"{OLLAMA_BASE_URL}/api/tags")
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
