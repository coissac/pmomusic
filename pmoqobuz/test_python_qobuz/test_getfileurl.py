#!/usr/bin/env python3
"""
Test simplifié - va directement à track_getFileUrl
Pour utiliser avec le fake server
"""

import sys
import getpass
from raw import RawApi

def main():
    print("=== Test track_getFileUrl (Python) ===\n")

    # Credentials
    username = input("Qobuz username (default: eric@coissac.eu): ").strip() or "eric@coissac.eu"
    password = getpass.getpass("Qobuz password: ")

    if not password:
        print("Error: password required")
        return 1

    # Track ID connu (récupéré du test précédent)
    track_id = "19557883"
    format_id = 27

    print("1. Creating RawApi (auto-initializes Spoofer)...")
    api = RawApi(appid=None, configvalue=None)
    print(f"   App ID: {api.appid}")

    print("\n2. Logging in...")
    login_result = api.user_login(username=username, password=password)
    if not login_result:
        print(f"   ✗ Login failed: {api.error}")
        return 1
    print(f"   ✓ Login successful - User ID: {api.user_id}")
    print(f"   Token: {api.user_auth_token[:20]}...")

    # Appel direct à track_getFileUrl
    print(f"\n3. Calling track_getFileUrl...")
    print(f"   Track ID: {track_id}")
    print(f"   Format ID: {format_id}")
    print(f"   ⚠️  THIS CALL IS SIGNED - Watch fake server logs!\n")

    file_url_data = api.track_getFileUrl(track_id=track_id, format_id=format_id)

    if not file_url_data:
        print(f"   ✗ Failed: {api.error}")
        print(f"   Status code: {api.status_code}")
        return 1

    url = file_url_data.get('url', '')
    mime_type = file_url_data.get('mime_type', '')

    print(f"   ✓ Success!")
    print(f"   URL: {url[:80]}...")
    print(f"   MIME type: {mime_type}")

    print("\n=== Test completed successfully! ===")
    return 0

if __name__ == "__main__":
    sys.exit(main())
