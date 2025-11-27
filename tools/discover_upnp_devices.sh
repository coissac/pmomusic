#!/bin/bash
#
# Script de découverte des devices UPnP via SSDP
#
# Usage: ./discover_upnp_devices.sh [timeout_seconds]

set -e

TIMEOUT="${1:-5}"

echo "=== Découverte des devices UPnP ===" >&2
echo "Timeout: ${TIMEOUT}s" >&2
echo >&2

# Créer un socket UDP pour envoyer la requête SSDP
DISCOVERY_MESSAGE="M-SEARCH * HTTP/1.1\r
Host: 239.255.255.250:1900\r
Man: \"ssdp:discover\"\r
MX: ${TIMEOUT}\r
ST: upnp:rootdevice\r
\r
"

# Envoyer la requête SSDP et collecter les réponses
echo "Envoi de la requête SSDP..." >&2
echo >&2

# Utiliser Python pour écouter les réponses SSDP
python3 - <<'PYTHON_SCRIPT' "$TIMEOUT"
import socket
import sys
import time

timeout = int(sys.argv[1])

# Créer un socket UDP
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.settimeout(timeout)

# Message SSDP M-SEARCH
msg = (
    'M-SEARCH * HTTP/1.1\r\n'
    'Host: 239.255.255.250:1900\r\n'
    'Man: "ssdp:discover"\r\n'
    'MX: {}\r\n'
    'ST: upnp:rootdevice\r\n'
    '\r\n'
).format(timeout)

# Envoyer la requête au multicast SSDP
sock.sendto(msg.encode(), ('239.255.255.250', 1900))

print("Écoute des réponses SSDP...", file=sys.stderr)
print(file=sys.stderr)

devices = {}
start_time = time.time()

try:
    while time.time() - start_time < timeout:
        try:
            data, addr = sock.recvfrom(8192)
            response = data.decode('utf-8', errors='ignore')

            # Extraire l'URL de la description
            location = None
            server = None
            usn = None
            for line in response.split('\r\n'):
                if line.lower().startswith('location:'):
                    location = line.split(':', 1)[1].strip()
                elif line.lower().startswith('server:'):
                    server = line.split(':', 1)[1].strip()
                elif line.lower().startswith('usn:'):
                    usn = line.split(':', 1)[1].strip()

            if location and location not in devices:
                devices[location] = {
                    'addr': addr[0],
                    'server': server,
                    'usn': usn
                }
                print(f"Trouvé: {location}", file=sys.stderr)
        except socket.timeout:
            break
except KeyboardInterrupt:
    pass
finally:
    sock.close()

print(file=sys.stderr)
print(f"=== {len(devices)} device(s) trouvé(s) ===", file=sys.stderr)
print(file=sys.stderr)

# Afficher les détails de chaque device
for location, info in devices.items():
    print(f"Device: {location}", file=sys.stderr)
    print(f"  IP: {info['addr']}", file=sys.stderr)
    if info['server']:
        print(f"  Server: {info['server']}", file=sys.stderr)
    if info['usn']:
        print(f"  USN: {info['usn']}", file=sys.stderr)

    # Récupérer la description XML
    import urllib.request
    try:
        with urllib.request.urlopen(location, timeout=2) as response:
            xml = response.read().decode('utf-8')

            # Parser le XML pour trouver les services
            import xml.etree.ElementTree as ET
            root = ET.fromstring(xml)

            # Namespaces UPnP
            ns = {
                'device': 'urn:schemas-upnp-org:device-1-0',
                'service': 'urn:schemas-upnp-org:service-1-0'
            }

            # Trouver le nom du device
            device_name = root.find('.//device:friendlyName', ns)
            if device_name is not None:
                print(f"  Name: {device_name.text}", file=sys.stderr)

            # Trouver le ContentDirectory service
            for service in root.findall('.//device:service', ns):
                service_type = service.find('device:serviceType', ns)
                if service_type is not None and 'ContentDirectory' in service_type.text:
                    control_url = service.find('device:controlURL', ns)
                    if control_url is not None:
                        # Construire l'URL complète
                        from urllib.parse import urljoin
                        full_control_url = urljoin(location, control_url.text)
                        print(f"  ContentDirectory Control URL: {full_control_url}", file=sys.stderr)
                        print(full_control_url)  # Output pour utilisation dans scripts
    except Exception as e:
        print(f"  Erreur lors de la récupération de la description: {e}", file=sys.stderr)

    print(file=sys.stderr)

PYTHON_SCRIPT
