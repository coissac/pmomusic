#!/usr/bin/env python3
"""
Fake Qobuz server for debugging
Logs all incoming requests with full details
"""

from http.server import HTTPServer, BaseHTTPRequestHandler
import json
import urllib.parse
from datetime import datetime

class FakeQobuzHandler(BaseHTTPRequestHandler):
    def log_request_details(self, method):
        """Log complete request details"""
        timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]

        print(f"\n{'='*80}")
        print(f"[{timestamp}] {method} {self.path}")
        print(f"{'='*80}")

        # Parse URL
        parsed = urllib.parse.urlparse(self.path)
        print(f"\nURL Components:")
        print(f"  Path: {parsed.path}")
        print(f"  Query: {parsed.query}")

        if parsed.query:
            params = urllib.parse.parse_qs(parsed.query)
            print(f"\nQuery Parameters:")
            for key, values in sorted(params.items()):
                print(f"  {key}: {values[0]}")

        # Headers
        print(f"\nHeaders:")
        for header, value in sorted(self.headers.items()):
            print(f"  {header}: {value}")

        # Body for POST
        if method == "POST":
            content_length = int(self.headers.get('Content-Length', 0))
            if content_length > 0:
                body = self.rfile.read(content_length)
                print(f"\nBody (raw): {body}")

                content_type = self.headers.get('Content-Type', '')
                if 'application/x-www-form-urlencoded' in content_type:
                    params = urllib.parse.parse_qs(body.decode('utf-8'))
                    print(f"\nForm Data:")
                    for key, values in sorted(params.items()):
                        print(f"  {key}: {values[0]}")
                elif 'application/json' in content_type:
                    try:
                        data = json.loads(body.decode('utf-8'))
                        print(f"\nJSON Data:")
                        print(f"  {json.dumps(data, indent=2)}")
                    except:
                        pass

        print(f"\n{'='*80}\n")

    def do_GET(self):
        self.log_request_details("GET")
        self.send_fake_response()

    def do_POST(self):
        self.log_request_details("POST")
        self.send_fake_response()

    def send_fake_response(self):
        """Send a fake successful response"""
        path = urllib.parse.urlparse(self.path).path

        # Login response
        if '/user/login' in path:
            response = {
                "user": {
                    "id": "1217710",
                    "credential": {
                        "parameters": {
                            "short_label": "Studio"
                        }
                    }
                },
                "user_auth_token": "FAKE_TOKEN_12345"
            }

        # Favorite albums
        elif '/favorite/getUserFavorites' in path:
            response = {
                "albums": {
                    "total": 1,
                    "items": [{
                        "id": "0825646206179",
                        "title": "Under the Shade of Violets",
                        "artist": {"name": "Orange Blossom"}
                    }]
                }
            }

        # Album get
        elif '/album/get' in path:
            response = {
                "tracks": {
                    "items": [{
                        "id": "12345678",
                        "title": "Test Track"
                    }]
                }
            }

        # Track getFileUrl
        elif '/track/getFileUrl' in path:
            response = {
                "url": "https://fake.qobuz.com/track.flac",
                "mime_type": "audio/flac",
                "sampling_rate": 44.1,
                "bit_depth": 16,
                "format_id": 27
            }

        else:
            response = {"status": "ok"}

        # Send response
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode('utf-8'))

    def log_message(self, format, *args):
        """Suppress default logging"""
        pass

def run_server(port=8080):
    server = HTTPServer(('localhost', port), FakeQobuzHandler)
    print(f"🎭 Fake Qobuz Server running on http://localhost:{port}")
    print(f"   Logging all requests to console...")
    print(f"   Press Ctrl+C to stop\n")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n\n✓ Server stopped")

if __name__ == "__main__":
    run_server()
