#!/bin/bash
set -e

# Lancer Ollama en arrière-plan
export OLLAMA_MODELS=/models
echo "🔹 Démarrage de Ollama..." 1>&2
ollama serve | sed 's/^/ 🔹[Ollama server] /'  &

sleep 10

echo "🔹 Preaload Ollama models: "
ollama ls  | sed 's/^/ 🔹 /' 1>&2

# Attendre Ollama
sleep 5

# Vérifier / précharger le modèle Nomic Embed Text
EMBED_MODEL="nomic-embed-text:latest"

echo "🔹 Vérification du modèle d'embedding: $EMBED_MODEL" 1>&2
if ! ollama list | grep -q "$EMBED_MODEL"; then
    echo " 🔹 Modèle $EMBED_MODEL non trouvé, téléchargement..." 1>&2
    ollama pull "$EMBED_MODEL"
else
    echo " 🔹 Modèle $EMBED_MODEL déjà présent" 1>&2
fi

echo "🔹 Vérification du modèle LLM: $OLLAMA_MODEL" 1>&2
if ! ollama list | grep -q "$OLLAMA_MODEL"; then
    echo " 🔹 Modèle $OLLAMA_MODEL non trouvé, téléchargement..." 1>&2
    ollama pull "$OLLAMA_MODEL"
else
    echo " 🔹 Modèle $OLLAMA_MODEL déjà présent" 1>&2
fi


# Lancer FastAPI
echo "🔹 Démarrage de FastAPI..." 1>&2
exec uvicorn app:app --host 0.0.0.0 --port 8000 --reload
