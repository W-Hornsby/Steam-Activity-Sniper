import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = 8765
HELLO_PATH = "/hello"
CALLBACK_PATH = "/callback"


class TampermonkeyPush:
    def __init__(self, on_callback, on_hello):
        self._on_callback = on_callback
        self._on_hello = on_hello
        self.state = "disconnected"

    def notify_hello(self):
        self.state = "connected"
        self._on_hello()

    def notify_callback(self, data):
        self._on_callback(data)


class _Handler(BaseHTTPRequestHandler):
    def __init__(self, *args, push=None, **kwargs):
        self._push = push
        super().__init__(*args, **kwargs)

    def _send(self, status, payload):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self._send(200, {})

    def do_GET(self):
        if self.path == "/status":
            self._send(200, {"state": self._push.state})
        else:
            self._send(404, {"error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        try:
            body = self.rfile.read(length)
            data = json.loads(body.decode("utf-8")) if body else {}
        except (ValueError, UnicodeDecodeError):
            data = {}
        if self.path == HELLO_PATH:
            self._push.notify_hello()
            self._send(200, {"state": self._push.state})
        elif self.path == CALLBACK_PATH:
            self._push.notify_callback(data)
            self._send(200, {"ok": True})
        else:
            self._send(404, {"error": "not found"})

    def log_message(self, *args):
        pass


def start_listener(push):
    server = ThreadingHTTPServer(("127.0.0.1", PORT), lambda *a, **k: _Handler(*a, push=push, **k))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server
