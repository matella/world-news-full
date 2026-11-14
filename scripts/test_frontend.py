#!/usr/bin/env python3
"""HTTP integration checks against a built Next app and a disposable synthetic DB."""
import base64
import os
import socket
import shutil
import subprocess
import tempfile
import time
import uuid
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, build_opener, ProxyHandler

from test_persistence import DOCKER, run

ROOT = Path(__file__).resolve().parents[1]


def main():
    container = "worldnews-http-" + uuid.uuid4().hex[:12]
    server = None
    opener = build_opener(ProxyHandler({}))  # Local loopback only, no outbound requests.
    try:
        run("run", "-d", "--name", container, "-p", "127.0.0.1::5432", "-e",
            "POSTGRES_PASSWORD=synthetic-bootstrap-only", "postgres:18-alpine")
        for attempt in range(120):
            try:
                run("exec", container, "psql", "-U", "postgres", "-Atc", "SELECT 1")
                time.sleep(1)
                run("exec", container, "psql", "-U", "postgres", "-Atc", "SELECT 1")
                break
            except subprocess.CalledProcessError:
                time.sleep(0.5)
        else:
            raise RuntimeError("Disposable database did not start")
        run("exec", "-i", container, "psql", "-U", "postgres", "-v", "ON_ERROR_STOP=1",
            input=(ROOT / "fixtures/legacy-test-schema.sql").read_bytes())
        # The transitional reader/admin must not require an external publisher's status table.
        run("exec", container, "psql", "-U", "postgres", "-v", "ON_ERROR_STOP=1", "-c", "DROP TABLE news_status")
        db_port = run("port", container, "5432").decode().strip().rsplit(":", 1)[1]
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            web_port = sock.getsockname()[1]
        env = dict(os.environ, DATABASE_URL=f"postgresql://fixture_reader:synthetic-reader-only@127.0.0.1:{db_port}/postgres",
                   NEWS_ADMIN_USER="fixture-editor", NEWS_ADMIN_PASSWORD="synthetic-admin-only",
                   NEXT_TELEMETRY_DISABLED="1")
        base = f"http://127.0.0.1:{web_port}"

        def request(path, headers=None):
            try:
                response = opener.open(Request(base + path, headers=headers or {}), timeout=15)
            except HTTPError as error:
                response = error
            return response.status, response.read().decode(), response.headers

        with tempfile.TemporaryFile() as log:
            server = subprocess.Popen(["node", "node_modules/next/dist/bin/next", "start", "--hostname", "127.0.0.1", "--port", str(web_port)],
                                      cwd=ROOT / "frontend", env=env, stdout=log, stderr=log)
            for attempt in range(60):
                try:
                    if request("/recherche")[0] == 200:
                        break
                except (URLError, ConnectionError):
                    time.sleep(0.5)
            else:
                log.seek(0)
                raise RuntimeError(log.read().decode())

            status, body, _ = request("/")
            assert status == 200 and "Pas d’édition" in body
            assert request("/edition/2026-02-30")[0] == 404
            assert request("/edition/2026-01-10")[0] == 404
            assert request("/story/does-not-exist")[0] == 404
            status, _, headers = request("/admin")
            assert status == 401 and "Basic" in headers["WWW-Authenticate"]
            auth = "Basic " + base64.b64encode(b"fixture-editor:synthetic-admin-only").decode()
            admin_status, admin_body, _ = request("/admin", {"Authorization": auth})
            assert admin_status == 200 and "200 publications récentes" in admin_body
            assert "Processing queue" not in admin_body and "Awaiting synthesis" not in admin_body
            run("exec", container, "psql", "-U", "postgres", "-v", "ON_ERROR_STOP=1", "-c",
                "INSERT INTO published_stories(id,title,body,lang,origin_count,source_count,edition_date) VALUES ('fixture-story','Synthetic headline','Synthetic body. Attribution historique : Jarvis.','fr',1,1,'2026-01-10')")
            for path in ["/", "/edition/2026-01-10", "/story/fixture-story"]:
                code, html, _ = request(path)
                assert code == 200 and "Synthetic headline" in html
                assert "192.168.129.85" not in html and "/?ask=" not in html
                assert "Analyse complète par Jarvis" not in html
                if path == "/story/fixture-story":
                    assert "Synthetic body. Attribution historique : Jarvis." in html
            if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1":
                from playwright.sync_api import sync_playwright
                with sync_playwright() as playwright:
                    browser = playwright.chromium.launch(headless=True, executable_path=shutil.which("chromium"), args=["--no-sandbox"])
                    context = browser.new_context()
                    external = []
                    def guard(route):
                        if route.request.url.startswith(base + "/"):
                            route.continue_()
                        else:
                            external.append(route.request.url)
                            route.abort()
                    context.route("**/*", guard)
                    try:
                        page = context.new_page()
                        for path in ["/", "/edition/2026-01-10", "/story/fixture-story"]:
                            page.goto(base + path, wait_until="networkidle")
                            page.get_by_role("heading", name="Synthetic headline", exact=True).wait_for()
                            links = page.locator("a").evaluate_all("nodes => nodes.map(node => node.href)")
                            assert all("/?ask=" not in link and "192.168.129.85" not in link for link in links)
                            assert page.get_by_role("link", name="Analyse complète par Jarvis ↗", exact=True).count() == 0
                        assert all(url.startswith("https://fonts.googleapis.com/") for url in external), external
                        print("PASS: Chromium live/archive/story navigation without publisher links or requests; external requests blocked")
                    finally:
                        browser.close()
            run("stop", container)
            status, body, _ = request("/")
            # Next streaming can commit 200 before failure; Rust will own strict 503 semantics.
            assert status == 500 or (status == 200 and '"digest"' in body), f"Unexpected database outage response: {status}"
            assert "Pas d’édition" not in body and "synthetic-reader-only" not in body
            if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1":
                from playwright.sync_api import sync_playwright
                with sync_playwright() as playwright:
                    browser = playwright.chromium.launch(headless=True, executable_path=shutil.which("chromium"), args=["--no-sandbox"])
                    try:
                        page = browser.new_page(viewport={"width": 390, "height": 844})
                        page.goto(base + "/", wait_until="networkidle")
                        page.get_by_role("heading", name="Le journal est temporairement indisponible.").wait_for()
                        assert page.get_by_role("button", name="Réessayer").is_visible()
                        print("PASS: Chromium mobile error boundary renders a visible retry action")
                    finally:
                        browser.close()
            print("PASS: empty edition, invalid/missing date/story, admin 401/authorised 200, read-only DB role, story read, unavailable DB UI")
    finally:
        if server:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
        subprocess.run(DOCKER + ["rm", "-f", "-v", container], capture_output=True)


if __name__ == "__main__":
    main()
