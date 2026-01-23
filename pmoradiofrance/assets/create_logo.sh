#!/bin/bash

# Create a simple PNG first, then convert to WebP
# Since we don't have image tools, we'll create a minimal valid WebP file

# Create a minimal 1x1 red WebP image (Radio France red: #e20613)
# This is a hex dump of a minimal WebP file
cat > radiofrance-logo.webp << 'WEBP'
UklGRiQAAABXRUJQVlA4IBgAAAAwAQCdASoBAAEAAwA0JaQAA3AA/vuUAAA=
WEBP

# Decode from base64
base64 -d -i radiofrance-logo.webp > radiofrance-logo-tmp.webp 2>/dev/null
mv radiofrance-logo-tmp.webp radiofrance-logo.webp 2>/dev/null || true

echo "WebP placeholder created"
