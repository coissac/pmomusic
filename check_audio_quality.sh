#!/bin/bash
# Script pour vérifier la qualité audio (détection de clics via le ratio delta)

CACHE_DIR="${1:-/tmp/pmomusic_test/audio_cache}"
THRESHOLD=10.0  # Ratio Maximum delta / Mean delta acceptable

if [ ! -d "$CACHE_DIR" ]; then
    echo "Error: Cache directory not found: $CACHE_DIR"
    exit 1
fi

echo "=== Audio Quality Check ==="
echo "Scanning: $CACHE_DIR"
echo "Threshold: Maximum/Mean delta ratio < $THRESHOLD"
echo ""

# Vérifier que sox est installé
if ! command -v sox &> /dev/null; then
    echo "Error: sox is not installed. Install it with: sudo apt install sox"
    exit 1
fi

count=0
suspicious=0
good=0

for file in "$CACHE_DIR"/*.orig.flac; do
    if [ ! -f "$file" ]; then
        echo "No FLAC files found in $CACHE_DIR"
        exit 0
    fi

    filename=$(basename "$file")

    # Obtenir les stats delta
    stats=$(sox "$file" -n stat 2>&1)

    max_delta=$(echo "$stats" | grep "Maximum delta" | awk '{print $3}')
    mean_delta=$(echo "$stats" | grep "Mean    delta" | awk '{print $3}')

    if [ -z "$max_delta" ] || [ -z "$mean_delta" ]; then
        echo "❌ $filename - Cannot parse stats"
        suspicious=$((suspicious + 1))
        count=$((count + 1))
        continue
    fi

    # Éviter division par zéro
    if (( $(echo "$mean_delta == 0" | bc -l) )); then
        echo "❌ $filename - Invalid mean delta (0)"
        suspicious=$((suspicious + 1))
        count=$((count + 1))
        continue
    fi

    # Calculer le ratio
    ratio=$(echo "scale=2; $max_delta / $mean_delta" | bc -l)

    # Comparer au seuil
    is_bad=$(echo "$ratio > $THRESHOLD" | bc -l)

    if [ "$is_bad" = "1" ]; then
        echo "⚠️  CLICKS DETECTED: $filename"
        echo "    Max delta: $max_delta, Mean delta: $mean_delta, Ratio: ${ratio}x (threshold: ${THRESHOLD}x)"
        suspicious=$((suspicious + 1))
    else
        echo "✓  OK: $filename (ratio: ${ratio}x)"
        good=$((good + 1))
    fi

    count=$((count + 1))
done

echo ""
echo "=== Summary ==="
echo "Total files scanned: $count"
echo "✓ Good quality: $good"
echo "⚠️  Clicks detected: $suspicious"

if [ $suspicious -gt 0 ]; then
    echo ""
    echo "⚠️  Warning: $suspicious file(s) have clicks."
    echo "These files were likely encoded with the old buffer size (8)."
    echo "Delete the cache and re-download to fix: rm -rf $CACHE_DIR/*.flac"
    exit 1
fi

echo ""
echo "✓ All files are good quality!"
exit 0
