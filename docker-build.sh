#!/bin/bash
# Script de build Docker pour PMOMusic
# Usage: ./docker-build.sh [OPTIONS]

set -e

# Couleurs pour l'affichage
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Configuration par défaut
IMAGE_NAME="pmomusic"
TAG="latest"
NO_CACHE=false
PUSH=true
REGISTRY="niepce.petite-maison-orange.fr/public"

# Fonction d'aide
show_help() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -t, --tag TAG         Tag de l'image (défaut: latest)"
    echo "  -r, --registry URL    Registry Docker (ex: ghcr.io/user)"
    echo "  -n, --no-cache        Build sans cache"
    echo "  -p, --push            Push l'image vers le registry"
    echo "  -h, --help            Affiche cette aide"
    echo ""
    echo "Exemples:"
    echo "  $0                                    # Build local avec tag 'latest'"
    echo "  $0 -t v1.0.0                         # Build avec tag 'v1.0.0'"
    echo "  $0 -t v1.0.0 -r ghcr.io/user -p      # Build et push vers GHCR"
    echo "  $0 -n                                # Build sans cache"
    exit 0
}

# Parsing des arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--tag)
            TAG="$2"
            shift 2
            ;;
        -r|--registry)
            REGISTRY="$2"
            shift 2
            ;;
        -n|--no-cache)
            NO_CACHE=true
            shift
            ;;
        -p|--push)
            PUSH=true
            shift
            ;;
        -h|--help)
            show_help
            ;;
        *)
            echo -e "${RED}Erreur: Option inconnue '$1'${NC}"
            show_help
            ;;
    esac
done

# Construire le nom complet de l'image
if [ -n "$REGISTRY" ]; then
    FULL_IMAGE_NAME="$REGISTRY/$IMAGE_NAME:$TAG"
else
    FULL_IMAGE_NAME="$IMAGE_NAME:$TAG"
fi

# Afficher la configuration
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Build Docker PMOMusic${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "Image: $FULL_IMAGE_NAME"
echo "No cache: $NO_CACHE"
echo "Push: $PUSH"
echo ""

# Construire la commande Docker
DOCKER_CMD="docker build"

if [ "$NO_CACHE" = true ]; then
    DOCKER_CMD="$DOCKER_CMD --no-cache"
fi

DOCKER_CMD="$DOCKER_CMD -t $FULL_IMAGE_NAME ."

# Exécuter le build
echo -e "${YELLOW}→ Démarrage du build...${NC}"
echo "Commande: $DOCKER_CMD"
echo ""

if eval "$DOCKER_CMD"; then
    echo ""
    echo -e "${GREEN}✓ Build réussi !${NC}"

    # Afficher la taille de l'image
    IMAGE_SIZE=$(docker images "$FULL_IMAGE_NAME" --format "{{.Size}}")
    echo "Taille de l'image: $IMAGE_SIZE"

    # Push si demandé
    if [ "$PUSH" = true ]; then
        echo ""
        echo -e "${YELLOW}→ Push de l'image vers le registry...${NC}"

        if docker push "$FULL_IMAGE_NAME"; then
            echo -e "${GREEN}✓ Image pushée avec succès !${NC}"
        else
            echo -e "${RED}✗ Erreur lors du push${NC}"
            exit 1
        fi
    fi

    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}Build terminé avec succès !${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo "Pour lancer le conteneur:"
    echo "  docker run -it --rm --network host $FULL_IMAGE_NAME"
    echo ""
    echo "Ou avec docker-compose:"
    echo "  docker-compose up -d"

else
    echo ""
    echo -e "${RED}✗ Erreur lors du build${NC}"
    exit 1
fi
