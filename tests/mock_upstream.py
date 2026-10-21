#!/usr/bin/env python3
"""Mock OpenAI-compatible upstream used to test the HermesRust proxy."""
import json, time
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _json(self, obj, code=200):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        if self.path.endswith("/models"):
            self._json({"object": "list", "data": [{"id": "mock-large"}]})
        else:
            self._json({"ok": True})

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(n) or b"{}")
        auth = self.headers.get("Authorization", "")
        # Capture what the proxy sent for the test to inspect
        with open("/home/z/my-project/upload/mock_upstream_last.json", "w") as f:
            json.dump({"auth": auth, "body": body}, f)

        if body.get("stream"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            chunks = ["Hello ", "from ", "the ", "**mock team**.", "\n\n```rust\nfn main() {}\n```",
                      "\n\n<!-- agent:agent-frontend -->", "\n\nDone."]
            # role chunk
            first = {"id": "cmpl-mock", "object": "chat.completion.chunk", "created": int(time.time()),
                     "model": body.get("model", "mock"), "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": None}]}
            self.wfile.write(f"data: {json.dumps(first)}\n\n".encode()); self.wfile.flush()
            for c in chunks:
                time.sleep(0.05)
                ch = {"id": "cmpl-mock", "object": "chat.completion.chunk", "created": int(time.time()),
                      "model": body.get("model", "mock"), "choices": [{"index": 0, "delta": {"content": c}, "finish_reason": None}]}
                # deliberately split one frame across two writes to test re-framing
                raw = f"data: {json.dumps(ch)}\n\n".encode()
                half = len(raw) // 2
                self.wfile.write(raw[:half]); self.wfile.flush()
                time.sleep(0.02)
                self.wfile.write(raw[half:]); self.wfile.flush()
            self.wfile.write(b"data: [DONE]\n\n"); self.wfile.flush()
        else:
            self._json({"id": "cmpl-mock", "object": "chat.completion", "created": int(time.time()),
                        "model": body.get("model", "mock"),
                        "choices": [{"index": 0, "message": {"role": "assistant", "content": "mock ok"}, "finish_reason": "stop"}]})

HTTPServer(("127.0.0.1", 9999), Handler).serve_forever()
