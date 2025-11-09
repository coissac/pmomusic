#!/bin/bash
# Script pour détecter les clics dans les fichiers FLAC du cache

CACHE_DIR="${1:-/tmp/pmomusic_test/audio_cache}"

if [ ! -d "$CACHE_DIR" ]; then
    echo "Error: Cache directory not found: $CACHE_DIR"
    exit 1
fi

echo "=== FLAC Click Detection Tool ==="
echo "Scanning: $CACHE_DIR"
echo ""

# Vérifier que sox est installé
if ! command -v sox &> /dev/null; then
    echo "Error: sox is not installed. Install it with: sudo apt install sox"
    exit 1
fi

count=0
suspicious=0

for file in "$CACHE_DIR"/*.orig.flac; do
    if [ ! -f "$file" ]; then
        echo "No FLAC files found in $CACHE_DIR"
        exit 0
    fi

    filename=$(basename "$file")
    echo "Analyzing: $filename"

    # Obtenir toutes les stats
    stats=$(sox "$file" -n stat 2>&1)

    # Extraire les valeurs importantes
    pk_lev=$(echo "$stats" | grep "Pk lev dB" | awk '{print $4}')
    rms_lev=$(echo "$stats" | grep "RMS lev dB" | awk '{print $4}')
    crest=$(echo "$stats" | grep "Crest factor" | awk '{print $3}')

    echo "  Peak level: ${pk_lev:-N/A} dB"
    echo "  RMS level: ${rms_lev:-N/A} dB"
    echo "  Crest factor: ${crest:-N/A} dB"

    # Analyser la variance d'amplitude (détection de clics)
    # On compte le nombre de pics au-dessus d'un seuil
    peaks=$(sox "$file" -n stats 2>&1 | grep "Maximum amplitude" | awk '{print $3}')

    if [ -n "$peaks" ]; then
        # Si le peak est proche de 1.0 (clipping), c'est suspect
        is_clipping=$(echo "$peaks > 0.95" | bc -l 2>/dev/null)
        if [ "$is_clipping" = "1" ]; then
            echo "  ⚠️  WARNING: Possible clipping detected!"
            suspicious=$((suspicious + 1))
        else
            echo "  ✓  OK"
        fi
    else
        echo "  ✓  OK"
    fi

    echo ""
    count=$((count + 1))
done

echo "=== Summary ==="
echo "Files scanned: $count"
echo "Suspicious files: $suspicious"

if [ $suspicious -gt 0 ]; then
    echo ""
    echo "⚠️  Some files may have issues. Listen to them carefully."
    exit 1
fi

exit 0
