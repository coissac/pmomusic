#!/usr/bin/env python3
"""
UPnP Device Discovery Tool
Envoie une requête M-SEARCH SSDP et collecte les réponses des devices
"""

import socket
import struct
import time
import sys
from urllib.parse import urlparse
from urllib.request import urlopen

SSDP_ADDR = "239.255.255.250"
SSDP_PORT = 1900
SSDP_MX = 3
SSDP_ST = "ssdp:all"

M_SEARCH = f"""M-SEARCH * HTTP/1.1
HOST: {SSDP_ADDR}:{SSDP_PORT}
MAN: "ssdp:discover"
MX: {SSDP_MX}
ST: {SSDP_ST}
USER-AGENT: PMOMusic UPnP Discovery Tool

"""

def discover_upnp_devices(timeout=5):
    """Découvre les devices UPnP sur le réseau local"""

    devices = {}

    # Créer le socket UDP
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.settimeout(timeout)

    # Envoyer la requête M-SEARCH
    print(f"🔍 Envoi de la requête M-SEARCH sur {SSDP_ADDR}:{SSDP_PORT}...")
    print(f"⏱️  Timeout: {timeout}s\n")

    message = M_SEARCH.replace('\n', '\r\n').encode('utf-8')
    sock.sendto(message, (SSDP_ADDR, SSDP_PORT))

    # Collecter les réponses
    start_time = time.time()

    while time.time() - start_time < timeout:
        try:
            data, addr = sock.recvfrom(65507)
            response = data.decode('utf-8', errors='ignore')

            # Parser la réponse
            location = None
            server = None
            st = None
            usn = None

            for line in response.split('\r\n'):
                if line.lower().startswith('location:'):
                    location = line.split(':', 1)[1].strip()
                elif line.lower().startswith('server:'):
                    server = line.split(':', 1)[1].strip()
                elif line.lower().startswith('st:'):
                    st = line.split(':', 1)[1].strip()
                elif line.lower().startswith('usn:'):
                    usn = line.split(':', 1)[1].strip()

            if location and location not in devices:
                devices[location] = {
                    'location': location,
                    'server': server,
                    'st': st,
                    'usn': usn,
                    'from': addr[0]
                }

        except socket.timeout:
            break
        except Exception as e:
            print(f"⚠️  Erreur lors de la réception: {e}")

    sock.close()
    return devices

def fetch_device_description(location):
    """Récupère la description XML du device"""
    try:
        response = urlopen(location, timeout=3)
        return response.read().decode('utf-8')
    except Exception as e:
        return f"Error: {e}"

def main():
    print("=" * 70)
    print(" 🔍 UPnP Device Discovery Tool")
    print("=" * 70)
    print()

    devices = discover_upnp_devices(timeout=5)

    # Filtrer pour ne garder que les MediaServers
    media_servers = {}
    for loc, info in devices.items():
        if 'MediaServer' in str(info.get('st', '')):
            media_servers[loc] = info

    print(f"\n📊 Résultats:")
    print(f"   Total devices trouvés: {len(devices)}")
    print(f"   MediaServers trouvés: {len(media_servers)}\n")

    if not media_servers:
        print("❌ Aucun MediaServer trouvé!\n")
        print("📋 Tous les devices trouvés:")
        for loc, info in devices.items():
            print(f"\n  - Location: {loc}")
            print(f"    ST: {info.get('st', 'N/A')}")
            print(f"    Server: {info.get('server', 'N/A')}")
        return

    # Analyser chaque MediaServer
    for idx, (location, info) in enumerate(media_servers.items(), 1):
        print("=" * 70)
        print(f"📡 MediaServer #{idx}")
        print("=" * 70)
        print(f"Location:  {location}")
        print(f"From IP:   {info['from']}")
        print(f"Server:    {info.get('server', 'N/A')}")
        print(f"USN:       {info.get('usn', 'N/A')}")
        print()

        # Récupérer la description
        print("📄 Fetching device description...")
        desc = fetch_device_description(location)

        # Analyser la description
        if desc and not desc.startswith("Error"):
            print("\n📝 Device Description XML:")
            print("-" * 70)
            # Afficher les premières lignes
            lines = desc.split('\n')
            for line in lines[:50]:  # Limiter à 50 lignes
                print(line)
            if len(lines) > 50:
                print(f"... ({len(lines) - 50} more lines)")
            print("-" * 70)

            # Extraire les infos importantes
            import re
            friendly_name = re.search(r'<friendlyName>([^<]+)</friendlyName>', desc)
            manufacturer = re.search(r'<manufacturer>([^<]+)</manufacturer>', desc)
            model_name = re.search(r'<modelName>([^<]+)</modelName>', desc)
            udn = re.search(r'<UDN>([^<]+)</UDN>', desc)

            print("\n📋 Device Info:")
            if friendly_name:
                print(f"   Friendly Name: {friendly_name.group(1)}")
            if manufacturer:
                print(f"   Manufacturer:  {manufacturer.group(1)}")
            if model_name:
                print(f"   Model Name:    {model_name.group(1)}")
            if udn:
                print(f"   UDN:           {udn.group(1)}")

                # Vérifier le format de l'UDN
                udn_value = udn.group(1)
                if not udn_value.startswith('uuid:'):
                    print(f"   ⚠️  WARNING: UDN ne commence pas par 'uuid:' !")
        else:
            print(f"❌ Erreur lors de la récupération: {desc}")

        print("\n")

if __name__ == "__main__":
    main()
