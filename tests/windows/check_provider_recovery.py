"""Resume a synthetic bridge-provider chat with the unmodified native engine.

Uses an isolated home, no credentials and no inference requests. Also checks
startup migration for a user who already disabled the old live-switch bridge.
"""
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
import tomllib
import winreg


class Engine:
    def __init__(self, executable, env):
        self.process = subprocess.Popen(
            [str(executable), "app-server", "--listen", "stdio://"],
            env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, encoding="utf-8",
            creationflags=0x08000000,
        )
        self.replies = queue.Queue()
        self.errors = []
        self.counter = 0
        def read():
            for line in self.process.stdout:
                self.replies.put(json.loads(line))
        def errors():
            for line in self.process.stderr:
                self.errors.append(line)
        threading.Thread(target=read, daemon=True).start()
        threading.Thread(target=errors, daemon=True).start()
        self.rpc("initialize", {"clientInfo": {"name": "meter_provider_regression", "version": "1"},
                                "capabilities": {"experimentalApi": True}})
        self.process.stdin.write('{"method":"initialized"}\n')
        self.process.stdin.flush()

    def rpc(self, method, params):
        self.counter += 1
        self.process.stdin.write(json.dumps({"id": self.counter, "method": method, "params": params}) + "\n")
        self.process.stdin.flush()
        while True:
            try:
                response = self.replies.get(timeout=30)
            except queue.Empty:
                raise AssertionError(f"No reply to {method}: {''.join(self.errors)[-2000:]}")
            if response.get("id") == self.counter:
                assert "error" not in response, response
                return response["result"]

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)


def user_launcher():
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment") as key:
        try:
            return winreg.QueryValueEx(key, "CODEX_CLI_PATH")
        except FileNotFoundError:
            return None


def run(meter, native):
    # A native engine can leave sandbox workers holding its temporary cwd briefly.
    with tempfile.TemporaryDirectory(prefix="meter-provider-regression-", ignore_cleanup_errors=True) as directory:
        root = Path(directory)
        home = root / "codex"
        home.mkdir()
        data = root / "local" / "com.codexmeter.app"
        launcher = data / "bridge" / "Codex-Meter-Bridge.exe"
        launcher.parent.mkdir(parents=True)
        shutil.copyfile(meter, launcher)
        original = '# Preserve my settings and comments\nmodel_reasoning_effort = "low"\n'
        config_path = home / "config.toml"
        config_path.write_text(original, encoding="utf-8")
        config = {"upstream": str(native), "upstreamArgs": [], "launcher": str(launcher),
                  "previousLauncher": None, "enabled": True}
        settings = data / "live-switch.json"
        settings.write_text(json.dumps(config), encoding="utf-8")
        env = {**os.environ, "CODEX_HOME": str(home), "LOCALAPPDATA": str(root / "local"),
               "APPDATA": str(root / "roaming"), "CODEX_CLI_PATH": str(native)}

        def assert_compatibility():
            text = config_path.read_text(encoding="utf-8")
            assert text.startswith(original), "Original settings were rewritten"
            parsed = tomllib.loads(text)
            assert "model_provider" not in parsed, "Default provider was changed"
            assert parsed["model_providers"]["meter-live"] == {
                "name": "OpenAI", "requires_openai_auth": True, "supports_websockets": False}
            assert not home.joinpath("auth.json").exists(), "Test touched credentials"
            return text

        bridge = Engine(launcher, env)
        try:
            started = bridge.rpc("thread/start", {"cwd": str(root), "modelProvider": "openai"})
            assert started["modelProvider"] == "meter-live", started
            thread_id = started["thread"]["id"]
            assert_compatibility()
        finally:
            bridge.close()

        # Native threads without a turn are deliberately not saved. Seed the
        # returned thread's rollout with a synthetic message instead of sending
        # a turn that could trigger inference. The provider is the bridge's
        # actual selection, and native resume reads it from the saved header.
        session = Path(started["thread"]["path"])
        session.parent.mkdir(parents=True, exist_ok=True)
        timestamp = "2026-10-08T00:00:00Z"
        records = [
            {"timestamp": timestamp, "type": "session_meta", "payload": {
                "id": thread_id, "timestamp": timestamp, "cwd": str(root),
                "originator": "meter_provider_regression", "cli_version": "0.151.0",
                "source": "cli", "model_provider": started["modelProvider"]}},
            {"timestamp": timestamp, "type": "response_item", "payload": {
                "type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "Synthetic saved-chat provider regression."}]}},
        ]
        session.write_text("\n".join(map(json.dumps, records)) + "\n", encoding="utf-8")

        # Disabling restores the normal launcher; the alias must outlive it.
        config["enabled"] = False
        settings.write_text(json.dumps(config), encoding="utf-8")
        normal = Engine(native, env)
        try:
            resumed = normal.rpc("thread/resume", {"threadId": thread_id, "cwd": str(root)})
            assert resumed["thread"]["id"] == thread_id
            assert resumed["modelProvider"] == "meter-live", resumed
            normal.rpc("thread/start", {"cwd": str(root), "modelProvider": "openai", "ephemeral": True})
        finally:
            normal.close()

        # Reproduce an old installation: disabled state plus missing provider.
        config_path.write_text(original, encoding="utf-8")
        before = user_launcher()
        startup = subprocess.STARTUPINFO()
        startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
        startup.wShowWindow = 0
        gui = subprocess.Popen([str(meter)], env=env, startupinfo=startup, creationflags=0x08000000)
        try:
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                assert gui.poll() is None, "Meter exited during startup repair"
                if "meter-live" in config_path.read_text(encoding="utf-8"):
                    break
                time.sleep(0.1)
            repaired = assert_compatibility()
            assert json.loads(settings.read_text())["enabled"] is False
            assert user_launcher() == before, "Startup repair re-enabled or changed the launcher"
            assert (home / "config.toml.before-meter-live.bak").read_text(encoding="utf-8") == original
        finally:
            gui.terminate()
            gui.wait(timeout=10)

        normal = Engine(native, env)
        try:
            assert normal.rpc("thread/resume", {"threadId": thread_id, "cwd": str(root)})["thread"]["id"] == thread_id
            assert config_path.read_text(encoding="utf-8") == repaired
        finally:
            normal.close()
        print("Native provider recovery passed: synthetic saved chat using the bridge's provider resumes normally after disabling, old disabled installation repairs on startup, default provider/settings/launcher preserved, no credentials or inference.")


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
