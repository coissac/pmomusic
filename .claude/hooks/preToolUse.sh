#!/bin/bash
# Bloquer toutes les éditions sans confirmation explicite
if [[ "$TOOL_NAME" == "Edit" ]] || [[ "$TOOL_NAME" == "MultiEdit" ]]; then
    echo "Édition bloquée - confirmation requise"
    exit 1
fi
