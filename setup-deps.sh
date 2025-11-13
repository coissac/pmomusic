#!/bin/bash
# Script d'installation automatique des dépendances soxr et alsa pour PMOMusic
# Usage: ./setup-deps.sh

set -e

echo "========================================="
echo "Installation des dépendances PMOMusic"
echo "========================================="
echo ""

# Créer le répertoire local
echo "1. Création du répertoire ~/.local"
mkdir -p ~/.local
cd ~/.local

# Télécharger les packages
echo ""
echo "2. Téléchargement des packages libsoxr et libasound2"
apt-get download libsoxr-dev libsoxr0 libasound2-dev libasound2t64

# Extraire les packages
echo ""
echo "3. Extraction des packages"
dpkg -x libsoxr-dev_*.deb .
dpkg -x libsoxr0_*.deb .
dpkg -x libasound2-dev_*.deb .
dpkg -x libasound2t64_*.deb .

# Vérifier l'installation
echo ""
echo "4. Vérification de l'installation"
if [ -f usr/lib/x86_64-linux-gnu/pkgconfig/soxr.pc ]; then
    echo "   ✓ libsoxr installé"
else
    echo "   ✗ Erreur: libsoxr non trouvé"
    exit 1
fi

if [ -f usr/lib/x86_64-linux-gnu/pkgconfig/alsa.pc ]; then
    echo "   ✓ libasound2 installé"
else
    echo "   ✗ Erreur: libasound2 non trouvé"
    exit 1
fi

# Retourner au projet
cd - > /dev/null

echo ""
echo "========================================="
echo "Installation terminée avec succès !"
echo "========================================="
echo ""
echo "Pour compiler le projet, exportez les variables d'environnement :"
echo ""
echo "  source setup-env.sh"
echo ""
echo "Puis compilez avec :"
echo ""
echo "  cargo build"
echo ""
