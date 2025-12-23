#!/usr/bin/env python3
"""
Script de test pour Qobuz API
Compare le comportement Python vs Rust
"""

import sys
import getpass
from spoofbuz import Spoofer
from raw import RawApi

def main():
    print("=== Test Qobuz API (Python) ===\n")

    # Obtenir les credentials
    username = input("Qobuz username (default: eric@coissac.eu): ").strip() or "eric@coissac.eu"
    password = getpass.getpass("Qobuz password: ")

    if not password:
        print("Error: password required")
        return 1

    print("\n1. Creating RawApi (will auto-initialize Spoofer)...")
    # Si on ne passe pas appid/configvalue, il crée automatiquement le spoofer
    api = RawApi(appid=None, configvalue=None)
    print(f"   App ID: {api.appid}")

    print("\n2. Logging in...")
    login_result = api.user_login(username=username, password=password)
    if not login_result:
        print(f"   ✗ Login failed: {api.error}")
        return 1
    print(f"   ✓ Login successful - User ID: {api.user_id}")

    print("\n3. Getting favorite albums...")
    albums = api.favorite_getUserFavorites(type="albums", limit="10")
    if not albums:
        print(f"   ✗ Failed: {api.error}")
        return 1

    album_count = albums.get('albums', {}).get('total', 0)
    print(f"   ✓ Found {album_count} favorite albums")

    # Get first album
    items = albums.get('albums', {}).get('items', [])
    if not items:
        print("   No albums in favorites")
        return 1

    first_album = items[0]
    album_id = first_album.get('id')
    album_title = first_album.get('title', 'Unknown')
    album_artist = first_album.get('artist', {}).get('name', 'Unknown')

    print(f"\n4. First album: {album_artist} - {album_title}")
    print(f"   Album ID: {album_id}")

    # Get album tracks
    print("\n5. Getting album tracks...")
    album_data = api.album_get(album_id=album_id)
    if not album_data:
        print(f"   ✗ Failed: {api.error}")
        return 1

    tracks = album_data.get('tracks', {}).get('items', [])
    if not tracks:
        print("   No tracks in album")
        return 1

    first_track = tracks[0]
    track_id = first_track.get('id')
    track_title = first_track.get('title', 'Unknown')
    print(f"   ✓ Found {len(tracks)} tracks")
    print(f"   First track: {track_title} (ID: {track_id})")

    # Get file URL (this is where signature is needed!)
    print("\n6. Getting file URL for first track...")
    print("   ⚠️  This call requires signature validation")

    file_url_data = api.track_getFileUrl(track_id=track_id, format_id=27)
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
