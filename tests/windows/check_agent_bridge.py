"""Exercise the packaged GUI executable's headless MCP/hook modes with fake quotas."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

guard = {"status": "pause", "shouldPause": True, "remainingPercent": 5,
         "checkpointId": "fixture:1791284400", "resumeAt": "2026-10-06T11:02:00+00:00",
         "prompt": "Save a handoff and schedule this same chat at 2026-10-06T11:02:00+00:00."}

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        assert self.path == "/api/guard"
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(guard).encode())
    def log_message(self, *_):
        pass

exe = str(Path(sys.argv[1]).resolve())
server = HTTPServer(("127.0.0.1", 32145), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with tempfile.TemporaryDirectory(prefix="meter-bridge-test-") as directory:
        env = {**os.environ, "CODEX_HOME": directory}
        requests = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "get_usage_guard", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "quota_checkpoint", "arguments": {"hook_event_name": "PreToolUse", "session_id": "mcp-chat", "turn_id": "turn"}}},
        ]
        result = subprocess.run([exe, "--mcp"], input="\n".join(map(json.dumps, requests))+"\n", text=True, capture_output=True, env=env, timeout=15, check=True)
        responses = list(map(json.loads, result.stdout.splitlines()))
        assert len(responses) == 4, result.stdout
        assert responses[1]["result"]["tools"][0]["name"] == "get_usage_guard"
        assert json.loads(responses[2]["result"]["content"][0]["text"])["remainingPercent"] == 5
        assert 'schedule this same chat' in json.loads(responses[3]["result"]["content"][0]["text"])["hookSpecificOutput"]["additionalContext"]
        event = {"hook_event_name": "PreToolUse", "session_id": "chat-a", "turn_id": "turn-a"}
        def hook(payload):
            output = subprocess.run([exe, "--quota-hook"], input=json.dumps(payload), text=True, capture_output=True, env=env, timeout=10, check=True)
            return json.loads(output.stdout)
        assert "11:02:00" in hook(event)["hookSpecificOutput"]["additionalContext"]
        assert hook(event) == {}, "Duplicate checkpoint warning"
        stop = {"hook_event_name": "Stop", "session_id": "chat-b", "turn_id": "turn-b"}
        assert hook(stop)["decision"] == "block"
        assert hook({**stop, "stop_hook_active": True}) == {}, "Stop hook loop"
        assert not Path(directory, "auth.json").exists(), "Bridge touched credentials"
    print("Packaged MCP handshake, usage tool, hook prompt, deduplication and no-credential checks passed")
finally:
    server.shutdown()
    server.server_close()
