"""Exercise the packaged stdio bridge with two synthetic accounts, never real logins."""
import base64
import ctypes
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid


def protect(data, encrypt):
    class Blob(ctypes.Structure):
        _fields_ = [("size", ctypes.c_ulong), ("data", ctypes.POINTER(ctypes.c_ubyte))]
    buffer = (ctypes.c_ubyte * len(data)).from_buffer_copy(data)
    source = Blob(len(data), buffer)
    output = Blob()
    method = ctypes.windll.crypt32.CryptProtectData if encrypt else ctypes.windll.crypt32.CryptUnprotectData
    assert method(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(output))
    try:
        return ctypes.string_at(output.data, output.size)
    finally:
        ctypes.windll.kernel32.LocalFree(output.data)


def login(name):
    claims = {"email": f"{name}@example.test", "exp": int(time.time()) + 3600,
              "https://api.openai.com/profile": {"email": f"{name}@example.test"},
              "https://api.openai.com/auth": {"chatgpt_account_id": name, "chatgpt_user_id": f"user-{name}", "chatgpt_plan_type": "plus"}}
    payload = base64.urlsafe_b64encode(json.dumps(claims).encode()).decode().rstrip("=")
    token = f"e30.{payload}.fixture"
    return {"auth_mode": "chatgpt", "tokens": {"id_token": token, "access_token": token, "refresh_token": f"refresh-{name}", "account_id": name}}


def profile(name):
    identity = json.dumps([name, f"user-{name}"], separators=(",", ":")).encode()
    return {"accountKey": f"chatgpt:{hashlib.sha256(identity).hexdigest()}", "label": f"{name}@example.test", "email": f"{name}@example.test", "planType": "plus", "isLegacy": False}


ENGINE = r'''
import json, os, sys
from pathlib import Path
home = Path(os.environ["CODEX_HOME"])
current = json.loads((home / "auth.json").read_text())["tokens"]["access_token"]
reject = False
def send(value):
 print(json.dumps(value), flush=True)
for line in sys.stdin:
 message = json.loads(line)
 method = message.get("method")
 params = message.get("params", {})
 result = {}
 if method == "initialize":
  result = {"userAgent": "fixture", "capabilities": params["capabilities"], "arguments": sys.argv[1:], "enginePid": os.getpid()}
 elif method == "getAuthStatus": result = {"authToken": current, "authMethod": "chatgptAuthTokens"}
 elif method == "account/read":
  import base64
  claims = json.loads(base64.urlsafe_b64decode(current.split('.')[1] + '=='))
  result = {"account": {"type": "chatgpt", "email": claims["email"], "planType": "plus"}}
 elif method == "account/login/start":
  if not reject: current = params["accessToken"]
  result = {"type": "chatgptAuthTokens"}
  send({"method": "account/updated", "params": {"authMode": "chatgptAuthTokens", "planType": "plus"}})
 elif method == "turn/start":
  result = {"turn": {"id": "active", "status": "inProgress"}}
  send({"method": "turn/started", "params": result})
 elif method == "fixture/finish": send({"method": "turn/completed", "params": {"turn": {"id": "active", "status": "completed"}}})
 elif method == "fixture/voice": send({"method": "thread/realtime/started", "params": {"threadId": "voice"}})
 elif method == "fixture/endVoice": send({"method": "thread/realtime/closed", "params": {"threadId": "voice"}})
 elif method == "fixture/reject": reject = params["enabled"]
 elif method == "fixture/late": send({"id": "meter-auth:late", "result": {"authToken": "must-stay-private"}})
 elif method == "fixture/pid": result = {"pid": os.getpid()}
 elif method == "fixture/refresh":
  send({"id": "renew", "method": "account/chatgptAuthTokens/refresh", "params": {"previousAccountId": "wrong-account", "reason": "unauthorized"}})
 elif not method:
  if message.get("id") == "renew": send({"method": "fixture/refreshResult", "params": {"hasError": "error" in message}})
  continue
 else: result = params
 if "id" in message: send({"id": message["id"], "result": result})
'''


def run(executable):
    with tempfile.TemporaryDirectory(prefix="meter-live-check-") as temporary:
        root = Path(temporary)
        data = root / "local" / "com.codexmeter.app"
        home = root / "codex"
        home.mkdir(parents=True)
        directory = data / "saved-logins"
        directory.mkdir(parents=True)
        saved = []
        fixtures = {name: login(name) for name in ("a", "b")}
        for name in ("a", "b"):
            identifier = str(uuid.uuid4())
            raw = json.dumps(fixtures[name]).encode()
            (directory / f"{identifier}.dpapi").write_bytes(protect(raw, True))
            saved.append({"id": identifier, "label": name.upper(), "account": profile(name), "savedAt": "2026-10-08T00:00:00Z"})
        (directory / "profiles.json").write_text(json.dumps(saved), encoding="utf-8")
        (home / "auth.json").write_text(json.dumps(fixtures["a"]), encoding="utf-8")
        launcher = data / "bridge" / "Codex-Meter-Bridge.exe"
        launcher.parent.mkdir(parents=True)
        shutil.copyfile(executable, launcher)
        engine = root / "engine.py"
        engine.write_text(ENGINE, encoding="utf-8")
        (data / "live-switch.json").write_text(json.dumps({"upstream": sys.executable, "upstreamArgs": [str(engine)], "launcher": str(launcher), "previousLauncher": None, "enabled": True}), encoding="utf-8")
        env = dict(os.environ, LOCALAPPDATA=str(root / "local"), CODEX_HOME=str(home))
        process = subprocess.Popen([str(launcher), "app-server", "--listen", "stdio://"], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
        replies = queue.Queue()
        notifications = []
        seen = []
        def read():
            for line in process.stdout:
                value = json.loads(line)
                seen.append(value)
                if "method" in value: notifications.append(value)
                else: replies.put(value)
        threading.Thread(target=read, daemon=True).start()
        counter = 0
        def rpc(method, params=None):
            nonlocal counter
            counter += 1
            process.stdin.write(json.dumps({"id": counter, "method": method, "params": params or {}}) + "\n")
            process.stdin.flush()
            response = replies.get(timeout=15)
            assert response["id"] == counter, response
            return response["result"]
        try:
            initialized = rpc("initialize", {"clientInfo": {"name": "desktop_fixture", "version": "1"}, "capabilities": {"experimentalApi": False, "optOutNotificationMethods": ["turn/completed", "unknown/notification"]}})
            assert initialized["capabilities"] == {"experimentalApi": True, "optOutNotificationMethods": ["unknown/notification"]}
            assert "model_providers.meter-live.supports_websockets=false" in initialized["arguments"]
            engine_pid = initialized["enginePid"]
            metadata_path = data / "bridge" / "connections" / f"{process.pid}.json"
            metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
            capability = protect(base64.b64decode(metadata["capability"]), False).decode()
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def control(path, body=None, authorized=True):
                headers = {"Content-Type": "application/json"}
                if authorized: headers["Authorization"] = f"Bearer {capability}"
                request = urllib.request.Request(f"http://127.0.0.1:{metadata['port']}/{path}", data=json.dumps(body).encode() if body else None, headers=headers)
                try:
                    with opener.open(request, timeout=60) as response: return response.status, json.loads(response.read())
                except urllib.error.HTTPError as error:
                    return error.code, json.loads(error.read() or b"{}")
            assert control("select", {"id": saved[1]["id"]}, False)[0] == 401
            assert control("status")[1]["account"]["accountKey"] == profile("a")["accountKey"]
            assert rpc("unknown/futureMethod", {"opaque": [1, "preserved"]}) == {"opaque": [1, "preserved"]}
            rpc("turn/start")
            assert control("select", {"id": saved[1]["id"]})[0] == 409
            assert json.loads((home / "auth.json").read_text())["tokens"]["account_id"] == "a"
            rpc("fixture/finish")
            rpc("fixture/voice")
            assert control("select", {"id": saved[1]["id"]})[0] == 409
            rpc("fixture/endVoice")
            # Reproduce the bug: disk already says B, desktop engine still says A.
            (home / "auth.json").write_text(json.dumps(fixtures["b"]), encoding="utf-8")
            rpc("fixture/reject", {"enabled": True})
            code, failed = control("select", {"id": saved[1]["id"]})
            assert code == 409 and "restored" in failed["error"], failed
            assert json.loads((home / "auth.json").read_text())["tokens"]["account_id"] == "a"
            rpc("fixture/reject", {"enabled": False})
            (home / "auth.json").write_text(json.dumps(fixtures["b"]), encoding="utf-8")
            code, switched = control("select", {"id": saved[1]["id"]})
            assert code == 200, switched
            assert switched["account"]["accountKey"] == profile("b")["accountKey"]
            assert rpc("getAuthStatus")["authToken"] == fixtures["b"]["tokens"]["access_token"]
            assert process.poll() is None
            rpc("fixture/late")
            assert not any(v.get("id", "").__str__().startswith("meter-auth:") for v in seen)
            rpc("fixture/refresh")
            deadline = time.monotonic() + 5
            while not any(v["method"] == "fixture/refreshResult" for v in notifications) and time.monotonic() < deadline: time.sleep(.05)
            assert any(v["method"] == "fixture/refreshResult" and v["params"]["hasError"] for v in notifications)
            assert not any(v.get("method") == "account/chatgptAuthTokens/refresh" for v in seen)
            rpc("fixture/reject", {"enabled": True})
            code, failed = control("select", {"id": saved[0]["id"]})
            assert code == 409 and "restored" in failed["error"], failed
            assert control("status")[1]["account"]["accountKey"] == profile("b")["accountKey"]
            assert json.loads((home / "auth.json").read_text())["tokens"]["account_id"] == "b"
            rpc("fixture/reject", {"enabled": False})
            assert control("select", {"id": saved[0]["id"]})[0] == 200
            assert rpc("getAuthStatus")["authToken"] == fixtures["a"]["tokens"]["access_token"]
            # The same upstream remains alive after both directions and recovery.
            assert rpc("fixture/pid")["pid"] == engine_pid and process.poll() is None
            assert any(v["method"] == "account/updated" for v in notifications)
            assert not any(v["method"] == "turn/completed" for v in notifications)
            print("Packaged live switching passed: A/B cached-file mismatch, engine confirmation, unchanged process, busy/voice guards, private RPCs, renewal routing, rollback, and transparent traffic.")
        finally:
            process.stdin.close()
            try: process.wait(timeout=8)
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=5)
            if process.returncode:
                print(process.stderr.read()[:1000], file=sys.stderr)


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve())
