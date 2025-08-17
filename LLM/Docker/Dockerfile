FROM python:3.11-slim

# Dépendances système
RUN apt-get update \
    && apt-get install -y \
        curl \
        libmagic1 \
        libxml2-dev \
        libxslt-dev \
        libjpeg-dev \
        zlib1g-dev \
        poppler-utils \
        tesseract-ocr \
        bash \
    && rm -rf /var/lib/apt/lists/*

# Installer Ollama
RUN curl -fsSL https://ollama.com/install.sh | sh

# Répertoires
RUN mkdir -p /app /chroma_db /response_cache /code
WORKDIR /app

# Copier le code
COPY requirements.txt .

# Installer Python requirements
RUN pip install --upgrade pip
RUN pip install --no-cache-dir -r requirements.txt

# Copier le code
COPY app.py .
COPY start.sh .
RUN chmod +x start.sh

# Variables d'environnement par défaut
ENV OLLAMA_API_URL="http://localhost:11434/api/generate"
ENV OLLAMA_MODEL="deepseek-coder:33b"
ENV CHROMA_PERSIST_DIR="/chroma_db"
ENV RESPONSE_CACHE_DIR="/response_cache"

# Exposer FastAPI
EXPOSE 8000

# Volumes persistants
VOLUME ["/models", "/code", "/chroma_db", "/response_cache"]

# Entrypoint
ENTRYPOINT ["/app/start.sh"]
