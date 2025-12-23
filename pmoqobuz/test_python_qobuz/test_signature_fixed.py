#!/usr/bin/env python3
"""
Test de signature MD5 avec timestamp fixe
Pour comparer avec Rust
"""

import hashlib
import sys
sys.path.insert(0, '.')
from spoofbuz import Spoofer

def main():
    print("=== Test de signature MD5 (Python) ===\n")

    # Récupérer le secret
    print("1. Getting secret from Spoofer...")
    spoofer = Spoofer()
    secrets = spoofer.getSecrets()
    # Utiliser le premier secret disponible
    app_secret = list(secrets.values())[0]
    print(f"   ✓ Secret retrieved (length: {len(app_secret)} chars)")

    # Paramètres du test
    track_id = "19557883"
    format_id = "27"
    intent = "stream"

    # TIMESTAMP FIXE pour comparaison
    timestamp = "1234567890.123456"

    print("\n2. Computing signature with FIXED timestamp:")
    print(f"   track_id: {track_id}")
    print(f"   format_id: {format_id}")
    print(f"   intent: {intent}")
    print(f"   timestamp: {timestamp}")
    print(f"   secret: {app_secret[:10]}... (first 10 chars)")

    # Calculer la signature (même logique que raw.py)
    stringvalue = (
        "trackgetFileUrlformat_id" + format_id +
        "intent" + intent +
        "track_id" + track_id +
        timestamp
    )
    stringvalue = stringvalue.encode("ASCII")
    stringvalue += app_secret.encode("utf-8")

    signature = hashlib.md5(stringvalue).hexdigest()

    print("\n3. Result:")
    print(f"   Signature: {signature}")
    print("\n✓ Compare this signature with Rust output")

if __name__ == "__main__":
    main()
