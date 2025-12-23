#!/bin/bash
# Script pour comparer Python vs Rust

echo "=== Qobuz API Comparison Test ==="
echo ""

# Check if fake server should be used
USE_FAKE=${1:-no}

if [ "$USE_FAKE" == "fake" ]; then
    echo "Mode: FAKE SERVER (debugging)"
    echo "1. Starting fake Qobuz server on port 8080..."

    # Start fake server
    python3 fake_qobuz_server.py &
    SERVER_PID=$!
    echo "   Server PID: $SERVER_PID"
    sleep 2

    # Patch raw.py
    echo "2. Patching raw.py to use localhost..."
    python3 patch_for_fake.py

    echo "3. Ready to test!"
    echo ""
    echo "Now run: python3 test_qobuz.py"
    echo ""
    echo "When done, press Enter to stop server and restore..."
    read

    # Cleanup
    echo "Stopping server..."
    kill $SERVER_PID 2>/dev/null
    echo "Restoring raw.py..."
    python3 patch_for_fake.py restore
    echo "Done!"

else
    echo "Mode: REAL API"
    echo ""
    echo "This will test against the real Qobuz API."
    echo "You'll need to enter your password."
    echo ""
    python3 test_qobuz.py
fi
