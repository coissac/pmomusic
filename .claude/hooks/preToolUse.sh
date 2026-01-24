#!/bin/bash

# Lire les données JSON envoyées par Claude Code
INPUT=$(cat)

# Extraire le nom de l'outil
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name')

# Pour les éditions de fichiers, forcer la demande de confirmation
if [[ "$TOOL_NAME" == "Edit" ]] || [[ "$TOOL_NAME" == "MultiEdit" ]] || [[ "$TOOL_NAME" == "Write" ]]; then
    # Retourner une décision "ask" qui force la confirmation
    cat << EOF
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "ask",
    "permissionDecisionReason": "Validation requise pour toute édition de fichier"
  }
}
EOF
    exit 0
fi

# Pour les autres outils, laisser passer normalement
exit 0
