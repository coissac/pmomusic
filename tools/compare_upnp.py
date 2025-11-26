#!/usr/bin/env python3
"""
Compare UPnP MediaServers
"""

from urllib.request import urlopen, Request
import re

# Devices à comparer
DEVICES = {
    "PMO Music 1": "http://192.168.0.138:8080/device/659878e3-9790-4ba0-a710-946e9470bd01/desc.xml",
    "PMO Music 2": "http://192.168.0.138:8080/device/8b8e9b19-9c65-4d59-b127-b34717658085/desc.xml",
    "Upmpdcli": "http://192.168.0.200:49152/uuid-c110358f-d885-b44a-d6d3-dca6329ead0d/description.xml",
    "Freebox": "http://192.168.0.254:52424/device.xml",
}

def fetch_description(url):
    """Récupère la description XML"""
    try:
        req = Request(url, headers={'User-Agent': 'PMOMusic/1.0'})
        response = urlopen(req, timeout=3)
        return response.read().decode('utf-8')
    except Exception as e:
        return f"Error: {e}"

def extract_info(xml):
    """Extrait les infos clés"""
    info = {}

    patterns = {
        'deviceType': r'<deviceType>([^<]+)</deviceType>',
        'friendlyName': r'<friendlyName>([^<]+)</friendlyName>',
        'manufacturer': r'<manufacturer>([^<]+)</manufacturer>',
        'modelName': r'<modelName>([^<]+)</modelName>',
        'UDN': r'<UDN>([^<]+)</UDN>',
        'specVersion': r'<specVersion>.*?<major>(\d+)</major>.*?<minor>(\d+)</minor>',
    }

    for key, pattern in patterns.items():
        match = re.search(pattern, xml, re.DOTALL)
        if match:
            if key == 'specVersion':
                info[key] = f"{match.group(1)}.{match.group(2)}"
            else:
                info[key] = match.group(1)

    # Extraire les services
    services = re.findall(r'<serviceType>([^<]+)</serviceType>', xml)
    info['services'] = services

    # Vérifier les icônes
    has_icons = bool(re.search(r'<iconList>', xml))
    info['hasIcons'] = has_icons

    return info

def main():
    print("=" * 100)
    print(" 🔍 UPnP MediaServer Comparison")
    print("=" * 100)
    print()

    results = {}

    for name, url in DEVICES.items():
        print(f"📡 Fetching {name}...")
        xml = fetch_description(url)

        if not xml.startswith("Error"):
            results[name] = {
                'xml': xml,
                'info': extract_info(xml)
            }
            print(f"   ✅ Fetched ({len(xml)} bytes)")
        else:
            print(f"   ❌ {xml}")
        print()

    # Comparer les résultats
    print("=" * 100)
    print(" 📊 COMPARISON")
    print("=" * 100)
    print()

    # Tableau comparatif
    print(f"{'Property':<20} | {'PMO Music 1':<30} | {'PMO Music 2':<30} | {'Upmpdcli':<30} | {'Freebox':<30}")
    print("-" * 150)

    properties = ['deviceType', 'specVersion', 'UDN', 'friendlyName', 'manufacturer', 'modelName', 'hasIcons']

    for prop in properties:
        row = f"{prop:<20} |"
        for device in ["PMO Music 1", "PMO Music 2", "Upmpdcli", "Freebox"]:
            if device in results:
                value = str(results[device]['info'].get(prop, 'N/A'))[:28]
                row += f" {value:<30} |"
            else:
                row += f" {'N/A':<30} |"
        print(row)

    print()
    print("=" * 100)
    print(" 🔌 SERVICES")
    print("=" * 100)
    print()

    for name, data in results.items():
        print(f"\n{name}:")
        for service in data['info'].get('services', []):
            print(f"  - {service}")

    # Afficher les XMLs complets pour PMO Music et un qui fonctionne
    print("\n" + "=" * 100)
    print(" 📄 FULL XML COMPARISON")
    print("=" * 100)

    if "PMO Music 1" in results:
        print("\n" + "=" * 50)
        print(" PMO Music MediaServer XML:")
        print("=" * 50)
        print(results["PMO Music 1"]['xml'])

    if "Upmpdcli" in results:
        print("\n" + "=" * 50)
        print(" Upmpdcli (WORKING) XML:")
        print("=" * 50)
        print(results["Upmpdcli"]['xml'])

    # Analyse des différences critiques
    print("\n" + "=" * 100)
    print(" ⚠️  CRITICAL DIFFERENCES")
    print("=" * 100)
    print()

    if "PMO Music 1" in results and "Upmpdcli" in results:
        pmo_udn = results["PMO Music 1"]['info'].get('UDN', '')
        upmp_udn = results["Upmpdcli"]['info'].get('UDN', '')

        print(f"UDN Format:")
        print(f"  PMO Music: {pmo_udn}")
        print(f"  Upmpdcli:  {upmp_udn}")

        if not pmo_udn.startswith('uuid:'):
            print(f"  ❌ PROBLÈME: PMO Music UDN ne commence pas par 'uuid:'")
        else:
            print(f"  ✅ PMO Music UDN format correct")

        if not upmp_udn.startswith('uuid:'):
            print(f"  ❌ PROBLÈME: Upmpdcli UDN ne commence pas par 'uuid:'")
        else:
            print(f"  ✅ Upmpdcli UDN format correct")

        print()

        pmo_icons = results["PMO Music 1"]['info'].get('hasIcons', False)
        upmp_icons = results["Upmpdcli"]['info'].get('hasIcons', False)

        print(f"Icons:")
        print(f"  PMO Music: {pmo_icons}")
        print(f"  Upmpdcli:  {upmp_icons}")

        if not pmo_icons and upmp_icons:
            print(f"  ⚠️  PMO Music n'a pas d'iconList (mais peut ne pas être critique)")

if __name__ == "__main__":
    main()
