"""Exercise a clean desktop profile using only the app's HTTP interface."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


binary = Path(sys.argv[1]).resolve()
base = "http://127.0.0.1:3000"


def request(path, password=None, body=None, method=None):
    headers = {"Content-Type": "application/json"}
    if password:
        headers["Authorization"] = "Bearer " + password
    req = Request(base + path, headers=headers, method=method,
                  data=None if body is None else json.dumps(body).encode())
    try:
        with urlopen(req, timeout=3) as response:
            return response.status, json.load(response)
    except HTTPError as error:
        return error.code, json.load(error)


def stop(process):
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=10)


with tempfile.TemporaryDirectory(prefix="ig-desktop-smoke-") as folder:
    cwd = Path(folder)
    profile = cwd / "profile"
    # Local mode must not depend on a project environment file or variable.
    (cwd / ".env.local").write_text("INVALID ' dotenv file", encoding="utf-8")
    env = dict(os.environ, IG_REPLY_BIND_ADDR="invalid", IG_ADMIN_PASSWORD="invalid")
    command = [str(binary), "--local", "--no-browser", "--data-dir", str(profile)]

    def start():
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL)
        try:
            for _ in range(100):
                if process.poll() is not None:
                    raise AssertionError("Local process exited during startup")
                try:
                    password = (profile / "admin-password.txt").read_text().strip()
                    code, body = request("/api/admin/status", password)
                    if code == 200:
                        return process, password, body
                except (OSError, URLError):
                    pass
                time.sleep(0.1)
            raise AssertionError("Local startup timed out")
        except BaseException:
            stop(process)
            raise

    process, password, status = start()
    try:
        assert status["keyword"] == "bot"
        assert status["automation_enabled"] is False
        assert status["configured"]["connected"] is False
        assert request("/api/admin/status")[0] == 401
        assert request("/api/admin/status", "wrong-password")[0] == 401
        settings = dict(keyword="hello", public_reply_text="Public smoke reply",
                        private_reply_text="Private smoke reply", poll_enabled=True,
                        poll_interval_seconds=60, app_secret="", webhook_verify_token="")
        assert request("/api/admin/settings", password, settings, "PUT")[0] == 200
        assert request("/api/admin/automation", password, {"enabled": True}, "POST")[0] == 409
        second = subprocess.run(command, cwd=cwd, env=env, capture_output=True, timeout=10)
        assert second.returncode == 0, "Second launch did not reopen the existing app"
        assert process.poll() is None
        other = cwd / "other-profile"
        conflict = subprocess.run([str(binary), "--local", "--no-browser", "--data-dir", str(other)],
                                  cwd=cwd, env=env, capture_output=True, timeout=10)
        assert conflict.returncode != 0
        assert not (other / "bot.sqlite3").exists(), "Port conflict touched a second database"
    finally:
        stop(process)
    process, next_password, status = start()
    try:
        assert password == next_password
        assert status["keyword"] == "hello"
        assert status["public_reply_text"] == "Public smoke reply"
        assert status["private_reply_text"] == "Private smoke reply"
        assert status["automation_enabled"] is False
        assert status["live_test_passed"] is False
    finally:
        stop(process)
    missing_path = subprocess.run([str(binary), "--local", "--data-dir"],
                                  capture_output=True, timeout=10)
    assert missing_path.returncode == 2
print("PASS: clean startup, authenticated panel, settings, restart, relaunch, port conflict, no-account guard")
