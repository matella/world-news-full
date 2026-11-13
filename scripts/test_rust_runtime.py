#!/usr/bin/env python3
"""Exercise the real Rust binary/configuration/shutdown without Node or a database."""
import json
import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import ProxyHandler, build_opener

ROOT = Path(__file__).resolve().parents[1]


def main():
    binary = ROOT / "target/debug/world-news"
    if not binary.is_file():
        raise SystemExit("Run cargo build -p world-news --locked first")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    # Explicit allowlist: do not inherit provider credentials or production URLs.
    env = {key: value for key, value in os.environ.items() if key in ("PATH", "SYSTEMROOT", "WINDIR")}
    env.update(DATABASE_URL="postgres://fixture:synthetic-private@127.0.0.1:1/news",
               BIND_ADDR=f"127.0.0.1:{port}", PUBLIC_URL=f"http://127.0.0.1:{port}",
               APP_ENV="development")
    result = subprocess.run([binary, "--check-config"], env=env, capture_output=True, timeout=10)
    assert result.returncode == 0 and b"synthetic-private" not in result.stdout + result.stderr
    bad = dict(env, DATABASE_URL="https://fixture:synthetic-private@localhost/news")
    result = subprocess.run([binary, "--check-config"], env=bad, capture_output=True, timeout=10)
    assert result.returncode != 0 and b"synthetic-private" not in result.stdout + result.stderr
    opener = build_opener(ProxyHandler({}))

    def request(path):
        try:
            response = opener.open(f"http://127.0.0.1:{port}" + path, timeout=5)
        except HTTPError as error:
            response = error
        return response.status, response.read().decode()

    with tempfile.TemporaryFile() as log:
        process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
        try:
            for attempt in range(60):
                try:
                    status, body = request("/health/live")
                    assert status == 200 and json.loads(body)["status"] == "ok"
                    break
                except (URLError, ConnectionError):
                    time.sleep(0.1)
            else:
                raise RuntimeError("Rust process failed to become live")
            assert request("/health/ready")[0] == 503
            status, body = request("/")
            assert status == 503 and "Édition en préparation" in body
            assert request("/admin")[0] == 404
            process.terminate()
            code = process.wait(timeout=10)
            if os.name == "posix":
                assert code == 0, f"SIGTERM was not graceful: {code}"
            log.seek(0)
            assert b"synthetic-private" not in log.read()
            print("PASS: config validation/redaction, real process liveness, DB readiness 503, explicit preparation page, SIGTERM shutdown; no Node/provider required")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


if __name__ == "__main__":
    main()
