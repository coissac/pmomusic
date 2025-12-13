#!/usr/bin/env python3
"""
Affiche tous les secrets du Spoofer Python
"""

import sys
sys.path.insert(0, '.')
from spoofbuz import Spoofer

def main():
    print("=== Python Spoofer Secrets ===\n")

    spoofer = Spoofer()

    # App ID
    app_id = spoofer.getAppId()
    print(f"App ID: {app_id}\n")

    # Tous les secrets
    secrets = spoofer.getSecrets()
    print(f"Number of secrets: {len(secrets)}\n")

    for i, (tz, secret) in enumerate(secrets.items(), 1):
        print(f"Secret {i} (timezone: {tz}):")
        print(f"  Full value: {secret}")
        print(f"  Length: {len(secret)}")
        print()

if __name__ == "__main__":
    main()
