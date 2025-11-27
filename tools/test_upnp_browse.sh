#!/bin/bash
#
# Script de test pour les requêtes Browse UPnP ContentDirectory
#
# Usage: ./test_upnp_browse.sh [options]
#
# Options:
#   -u URL    URL de contrôle du service ContentDirectory
#   -o ID     Object ID à parcourir (défaut: "0")
#   -f FLAG   BrowseFlag: BrowseMetadata ou BrowseDirectChildren (défaut: BrowseDirectChildren)
#   -s INDEX  StartingIndex (défaut: 0)
#   -c COUNT  RequestedCount (défaut: 0 = tous)
#   -h        Afficher cette aide

set -e

# Valeurs par défaut
CONTROL_URL="http://localhost:8080/device/63623ff4-ee41-4850-90aa-2d39395df981/service/ContentDirectory/control"
OBJECT_ID="0"
BROWSE_FLAG="BrowseDirectChildren"
STARTING_INDEX=0
REQUESTED_COUNT=0

# Fonction d'aide
show_help() {
    cat << EOF
Script de test pour les requêtes Browse UPnP ContentDirectory

Usage: $0 [options]

Options:
  -u URL    URL de contrôle du service ContentDirectory
            (défaut: http://localhost:8080/device/.../ContentDirectory/control)
  -o ID     Object ID à parcourir (défaut: "0")
  -f FLAG   BrowseFlag: BrowseMetadata ou BrowseDirectChildren
            (défaut: BrowseDirectChildren)
  -s INDEX  StartingIndex (défaut: 0)
  -c COUNT  RequestedCount (défaut: 0 = tous)
  -h        Afficher cette aide

Exemples:
  # Parcourir la racine
  $0

  # Parcourir un canal Radio Paradise
  $0 -o "radio-paradise:channel:main"

  # Obtenir les métadonnées d'un container
  $0 -o "radio-paradise:channel:main:history" -f BrowseMetadata

  # Parcourir l'historique
  $0 -o "radio-paradise:channel:main:history"

EOF
}

# Parser les options
while getopts "u:o:f:s:c:h" opt; do
    case $opt in
        u) CONTROL_URL="$OPTARG" ;;
        o) OBJECT_ID="$OPTARG" ;;
        f) BROWSE_FLAG="$OPTARG" ;;
        s) STARTING_INDEX="$OPTARG" ;;
        c) REQUESTED_COUNT="$OPTARG" ;;
        h) show_help; exit 0 ;;
        \?) echo "Option invalide: -$OPTARG" >&2; show_help; exit 1 ;;
    esac
done

# Afficher les paramètres
echo "=== Test Browse UPnP ===" >&2
echo "Control URL: $CONTROL_URL" >&2
echo "Object ID: $OBJECT_ID" >&2
echo "Browse Flag: $BROWSE_FLAG" >&2
echo "Starting Index: $STARTING_INDEX" >&2
echo "Requested Count: $REQUESTED_COUNT" >&2
echo >&2

# Échapper l'Object ID pour XML
OBJECT_ID_ESCAPED=$(echo "$OBJECT_ID" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g; s/"/\&quot;/g; s/'"'"'/\&apos;/g')

# Construire et envoyer la requête SOAP
RESPONSE=$(curl -s -w "\nHTTP_STATUS:%{http_code}" -X POST "$CONTROL_URL" \
  -H "Content-Type: text/xml; charset=\"utf-8\"" \
  -H "SOAPACTION: \"urn:schemas-upnp-org:service:ContentDirectory:1#Browse\"" \
  -d "<?xml version=\"1.0\"?>
<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">
  <s:Body>
    <u:Browse xmlns:u=\"urn:schemas-upnp-org:service:ContentDirectory:1\">
      <ObjectID>$OBJECT_ID_ESCAPED</ObjectID>
      <BrowseFlag>$BROWSE_FLAG</BrowseFlag>
      <Filter>*</Filter>
      <StartingIndex>$STARTING_INDEX</StartingIndex>
      <RequestedCount>$REQUESTED_COUNT</RequestedCount>
      <SortCriteria></SortCriteria>
    </u:Browse>
  </s:Body>
</s:Envelope>")

# Extraire le code de statut HTTP
HTTP_STATUS=$(echo "$RESPONSE" | grep "HTTP_STATUS:" | cut -d: -f2)
BODY=$(echo "$RESPONSE" | sed '/HTTP_STATUS:/d')

echo "=== HTTP Status: $HTTP_STATUS ===" >&2
echo >&2

# Afficher la réponse formatée
if [ -n "$BODY" ]; then
    echo "=== SOAP Response ===" >&2
    echo "$BODY" | xmllint --format - 2>&1

    # Extraire et décoder le DIDL-Lite
    DIDL=$(echo "$BODY" | xmllint --xpath "string(//Result)" - 2>/dev/null || true)
    if [ -n "$DIDL" ]; then
        echo >&2
        echo "=== DIDL-Lite Content ===" >&2
        echo "$DIDL" | xmllint --format - 2>&1 || echo "$DIDL"
    fi

    # Afficher NumberReturned et TotalMatches
    echo >&2
    echo "=== Statistics ===" >&2
    NUMBER_RETURNED=$(echo "$BODY" | xmllint --xpath "string(//NumberReturned)" - 2>/dev/null || echo "N/A")
    TOTAL_MATCHES=$(echo "$BODY" | xmllint --xpath "string(//TotalMatches)" - 2>/dev/null || echo "N/A")
    UPDATE_ID=$(echo "$BODY" | xmllint --xpath "string(//UpdateID)" - 2>/dev/null || echo "N/A")
    echo "NumberReturned: $NUMBER_RETURNED" >&2
    echo "TotalMatches: $TOTAL_MATCHES" >&2
    echo "UpdateID: $UPDATE_ID" >&2
else
    echo "Aucune réponse du serveur" >&2
    exit 1
fi
