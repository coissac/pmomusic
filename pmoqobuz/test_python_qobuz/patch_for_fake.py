#!/usr/bin/env python3
"""
Patch raw.py to use fake server
"""

import sys

def patch_raw_py():
    """Replace base URL in raw.py"""
    with open('raw.py', 'r') as f:
        content = f.read()

    # Replace base URL (without version, as it's constructed dynamically)
    original_line = 'self.baseUrl = "https://www.qobuz.com/api.json/"'
    fake_line = 'self.baseUrl = "http://localhost:8080/api.json/"'

    if original_line in content:
        content = content.replace(original_line, fake_line)
        with open('raw.py', 'w') as f:
            f.write(content)
        print(f"✓ Patched raw.py:")
        print(f"  {original_line}")
        print(f"  → {fake_line}")
        return True
    else:
        print("✗ Original URL not found in raw.py")
        print("   Looking for:", original_line)
        return False

def unpatch_raw_py():
    """Restore original URL in raw.py"""
    with open('raw.py', 'r') as f:
        content = f.read()

    original_line = 'self.baseUrl = "https://www.qobuz.com/api.json/"'
    fake_line = 'self.baseUrl = "http://localhost:8080/api.json/"'

    if fake_line in content:
        content = content.replace(fake_line, original_line)
        with open('raw.py', 'w') as f:
            f.write(content)
        print(f"✓ Restored raw.py to original URL")
        return True
    else:
        print("✗ Fake URL not found in raw.py (already restored?)")
        return False

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "restore":
        unpatch_raw_py()
    else:
        patch_raw_py()
        print("\nTo restore: python3 patch_for_fake.py restore")
