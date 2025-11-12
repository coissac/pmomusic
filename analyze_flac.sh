#!/bin/bash
# Affiche toutes les stats d'un fichier FLAC

if [ $# -eq 0 ]; then
    echo "Usage: $0 <fichier.flac>"
    exit 1
fi

FILE="$1"

if [ ! -f "$FILE" ]; then
    echo "Error: File not found: $FILE"
    exit 1
fi

echo "=== Analyzing: $(basename "$FILE") ==="
echo ""
echo "--- SoX Statistics ---"
sox "$FILE" -n stat 2>&1

echo ""
echo "--- File Info ---"
file "$FILE"

echo ""
echo "--- FLAC Metadata ---"
metaflac --list "$FILE" 2>/dev/null || echo "metaflac not installed"

echo ""
echo "--- Audio Integrity Check ---"
flac -t "$FILE" 2>&1 || echo "flac not installed"
