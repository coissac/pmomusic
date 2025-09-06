#!/bin/bash
set -e

# Lancer Ollama en arrière-plan
# export OLLAMA_MODELS=/models
# echo "🔹 Démarrage de Ollama..." 1>&2
# ollama serve 2>&1 \
# | grep -vF "decode: cannot decode batches with this context (use llama_encode() instead)" \
# | sed 's/^/ 🔹[Ollama server] /' 1>&2 &

# sleep 10

if [[ -n "$1" ]] ; then 
    eval $*
fi

echo "🔹 Preaload Ollama models: "
ollama ls  | sed 's/^/ 🔹 /' 1>&2

# Vérifier / précharger le modèle d'embedding

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
