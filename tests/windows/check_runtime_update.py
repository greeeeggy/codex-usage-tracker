"""An existing cached engine must not pin the bridge across desktop updates.

Uses isolated storage, a real native CLI version command, and synthetic helper
bytes. It does not start inference, change the user's launcher, or access login.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run(meter, native):
    with tempfile.TemporaryDirectory(prefix="meter-runtime-update-") as directory:
        root = Path(directory)
        local = root / "local"
        home = root / "home"
        home.mkdir()
        old = local / "OpenAI" / "Codex" / "bin" / "old"
        new = old.with_name("new")
        for runtime, modified in ((old, 1_700_000_000), (new, 1_800_000_000)):
            runtime.mkdir(parents=True)
            shutil.copyfile(native, runtime / "codex.exe")
            os.utime(runtime / "codex.exe", (modified, modified))
        (old / "codex-code-mode-host.exe").write_bytes(b"old helper")
        (new / "codex-code-mode-host.exe").write_bytes(b"new helper")
        (new / "codex-future-helper.exe").write_bytes(b"future helper")
        data = local / "com.codexmeter.app"
        launcher = data / "bridge" / "Codex-Meter-Bridge.exe"
        launcher.parent.mkdir(parents=True)
        shutil.copyfile(meter, launcher)
        (data / "live-switch.json").write_text(json.dumps({
            "upstream": str(old / "codex.exe"), "upstreamArgs": [],
            "launcher": str(launcher), "previousLauncher": None, "enabled": True,
        }), encoding="utf-8")
        env = {**os.environ, "LOCALAPPDATA": str(local), "CODEX_HOME": str(home), "CODEX_CLI_PATH": str(launcher)}
        original = hashlib.sha256(launcher.read_bytes()).digest()
        for expected in (b"new helper", b"next update"):
            (new / "codex-code-mode-host.exe").write_bytes(expected)
            result = subprocess.run([str(launcher), "--version"], env=env, text=True,
                                    capture_output=True, timeout=30, check=True)
            assert result.stdout.strip().startswith("codex-cli "), result.stdout
            assert (launcher.parent / "codex-code-mode-host.exe").read_bytes() == expected
            assert (launcher.parent / "codex-future-helper.exe").read_bytes() == b"future helper"
        assert old.joinpath("codex.exe").exists(), "Test did not retain the stale engine"
        assert hashlib.sha256(launcher.read_bytes()).digest() == original
        assert not home.joinpath("auth.json").exists(), "Bridge accessed credentials"
        print("Packaged runtime update passed: stale engine retained, current helpers refreshed twice, future companion discovered, bridge preserved, no credentials or inference.")


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
