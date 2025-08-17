import os
import sys
import glob
import time
import hashlib
import textwrap
import requests
import httpx
from fastapi.responses import StreamingResponse
from fastapi import FastAPI, Query, HTTPException, Request
from langchain_community.vectorstores import Chroma
from langchain_ollama import OllamaEmbeddings
from langchain_community.document_loaders import DirectoryLoader, TextLoader
from langchain_text_splitters import RecursiveCharacterTextSplitter
from duckduckgo_search import DDGS
from unstructured.cleaners.core import clean_extra_whitespace, clean_non_ascii_chars, replace_unicode_quotes
from datetime import datetime, timezone, timedelta


# --- Configuration via variables d'environnement ---
PERSIST_DIR = os.environ.get("CHROMA_PERSIST_DIR", "/chroma_db")
CACHE_DIR = os.environ.get("RESPONSE_CACHE_DIR", "/response_cache")
OLLAMA_URL = os.environ.get("OLLAMA_URL", "http://127.0.0.1:11434")
MODEL_NAME = os.environ.get("OLLAMA_MODEL", "llama3:13b")

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
    """Wrapper automatique pour les préfixes Nomic"""
    
    def _prefix_text(self, text: str, is_document: bool) -> str:
        prefix = "search_document: " if is_document else "search_query: "
        return prefix + text
    
    def embed_documents(self, texts: List[str]) -> List[List[float]]:
        prefixed_texts = [self._prefix_text(t, is_document=True) for t in texts]
        return super().embed_documents(prefixed_texts)
    
    def embed_query(self, text: str) -> List[float]:
        return super().embed_query(self._prefix_text(text, is_document=False))

# --- FastAPI ---
app = FastAPI()

# --- Traitement des chemins ---
paths = sys.argv[1:] if len(sys.argv) > 1 else ["."]
if not paths:
    paths = ["."]

# --- Initialisation ---
vectorstore = None
code_hash = ""

def build_vectorstore():
    global vectorstore, code_hash
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
        chunk_overlap=150,
        separators=["\n\n", "\nfunc ", "}\n\n", "\n//", "\n/*", "\t"]
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
            loader_kwargs={'autodetect_encoding': True},
            max_files=500
        )
        loaded_docs = loader.load()
        print(f"   🔸 {len(loaded_docs)} fichiers chargés", file=sys.stderr)
        for doc in loaded_docs:
            doc.page_content = clean_code_content(doc.page_content)
        all_docs.extend(loaded_docs)

    print(f"🔹 {len(all_docs)} documents après chargement", file=sys.stderr)
    splits = go_splitter.split_documents(all_docs)
    print(f"🔹 {len(splits)} chunks créés", file=sys.stderr)

    embedding = NomicEmbeddingsWrapper(model="nomic-embed-text", api_base=OLLAMA_URL)

    # Créer ou recharger Chroma
    vectorstore = Chroma.from_documents(
        documents=splits,
        embedding=embedding,
        persist_directory=PERSIST_DIR,
        collection_metadata={"hnsw:space": "cosine"}
    )
    vectorstore.persist()
    print("🔹 Vectorstore créé et persisté", file=sys.stderr)

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

def build_prompt(question: str,
                 k_rag: int,
                 k_web: int):
    build_vectorstore()

    rag_docs = vectorstore.similarity_search(question, k=k_rag)
    context_str = format_context(rag_docs) if rag_docs else "Aucun contexte trouvé."

    # Recherche web
    web_info = ""
    if k_web > 0:
        try:
            with DDGS(timeout=10) as ddgs:
                results = list(ddgs.text(question, max_results=k_web))
                web_info = "\n".join(f"- [{r['title']}]({r['href']}): {r['body'][:150]}..." for r in results) if results else "Aucun résultat web trouvé."
        except Exception as e:
            web_info = f"⚠️ Erreur recherche web: {str(e)}"
    else:
        web_info = "Recherche web désactivée."

    prompt = f"""
Vous êtes un expert en programmation Go. Répondez à la question en utilisant le contexte fourni (extraits de code) et les informations web si disponibles.

**Contexte Code (extraits pertinents):**
{context_str}

**Informations Web:**
{web_info}

**Question:**
{question}

**Instructions:**
- Répondez de manière concise et précise
- Si la réponse se trouve dans le contexte code, citez le fichier et l'extrait correspondant
- Si vous utilisez les informations web, citez la source
- Si la question est en anglais, répondez en anglais. Sinon, en français
- Pour les extraits de code, conservez le formatage et l'indentation
"""

    return prompt

@app.post("/api/chat")
async def chat(
    question: str = Query(..., min_length=3),
    history: list = Query(default=[]),  # liste d'anciens messages [{role, content}]
    k_rag: int = Query(4, ge=1, le=10),
    k_web: int = Query(2, ge=0, le=5),
):
    start_time = time.time()

    # Construire le prompt enrichi (RAG + Web)
    prompt = build_prompt(question=question, k_rag=k_rag, k_web=k_web)

    # Construire la conversation pour Ollama
    messages = history + [
        {"role": "user", "content": prompt}
    ]

    try:
        r = requests.post(
            f"{OLLAMA_URL}/api/chat",
            json={
                "model": MODEL_NAME,
                "messages": messages,
                "options": {
                    "temperature": 0.3,
                    "num_predict": 1024,
                    "top_k": 50,
                    "top_p": 0.9
                }
            },
            timeout=120
        )
        r.raise_for_status()
        result = r.json()
        answer = result.get("message", {}).get("content", "Pas de réponse générée.")

        return {
            "answer": answer,
            "processing_time": f"{time.time() - start_time:.2f}s",
            "model": MODEL_NAME,
            "cached": False,
            "history": messages + [{"role": "assistant", "content": answer}]
        }
    except requests.exceptions.RequestException as e:
        detail = f"Erreur API Ollama: {str(e)}"
        if hasattr(e, 'response') and e.response:
            detail += f" | Status: {e.response.status_code} | Response: {e.response.text[:200]}"
        raise HTTPException(status_code=500, detail=detail)

# --- Endpoint /ask ---
@app.get("/api/generate")
async def ask_question(
    question: str = Query(..., min_length=3),
    k_rag: int = Query(4, ge=1, le=10),
    k_web: int = Query(2, ge=0, le=5),
    use_cache: bool = Query(True)
):
    start_time = time.time_ns()

    cache_path = os.path.join(CACHE_DIR, f"{get_cache_key(question)}.txt")
    if use_cache and os.path.exists(cache_path):
        with open(cache_path, "r") as f:
            return {"answer": f.read(), "cached": True}

    prompt = build_prompt(question=question, k_rag=k_rag, k_web=k_web)

    try:
        r = requests.post(f"{OLLAMA_URL}/api/generate", json={
            "model": MODEL_NAME,
            "prompt": prompt,
            "stream": False,
            "options": {"temperature": 0.3, "num_predict": 1024, "top_k": 50, "top_p": 0.9}
        }, timeout=120)
        r.raise_for_status()
        result = r.json()
        answer = result.get("response") or result.get("text") or "Pas de réponse générée."

        with open(cache_path, "w") as f:
            f.write(answer)

        return {
                "model": MODEL_NAME,
                "eval_duration": f"{time.time_ns() - start_time:.0f}",
                "created_at": format_iso_time_with_ns(),
                "response": answer,
                "done": false
               }
        
        
        {"answer": answer, 
                "processing_time": f"{time.time() - start_time:.2f}s", 
                "model": MODEL_NAME, 
                "cached": False}
    except requests.exceptions.RequestException as e:
        detail = f"Erreur API Ollama: {str(e)}"
        if hasattr(e, 'response') and e.response:
            detail += f" | Status: {e.response.status_code} | Response: {e.response.text[:200]}"
        raise HTTPException(status_code=500, detail=detail)

# --- Endpoint /status ---
@app.get("/status")
def status_check():
    try:
        count = vectorstore._collection.count() if vectorstore else 0
        return {
            "status": "OK",
            "vectorstore_items": count,
            "model": MODEL_NAME,
            "persist_dir": PERSIST_DIR,
            "cache_dir": CACHE_DIR
        }
    except Exception as e:
        raise HTTPException(500, f"Erreur: {str(e)}")

@app.api_route("/{path:path}", methods=["GET", "POST", "PUT", "DELETE", "PATCH"])
async def proxy(request: Request, path: str):
    async with httpx.AsyncClient() as client:
        url = f"{OLLAMA_URL}/{path}"
        body = await request.body()
        r = await client.request(
            method=request.method,
            url=url,
            headers=request.headers,
            content=body
        )
        return StreamingResponse(r.aiter_bytes(), status_code=r.status_code, headers=dict(r.headers))
