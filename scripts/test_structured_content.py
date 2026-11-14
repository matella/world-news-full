#!/usr/bin/env python3
"""Anonymous browser check of synthetic structured publication fixtures only."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_structure_reader:")
    assert "@127.0.0.1:" in reader_url and reader_url.endswith("_structured")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    env = dict(DATABASE_URL=reader_url, APP_ENV="development", PUBLIC_URL=origin,
               BIND_ADDR=f"127.0.0.1:{port}")
    with tempfile.TemporaryFile() as log:
        app = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
        try:
            opener = build_opener(ProxyHandler({}))
            for _ in range(60):
                try:
                    assert opener.open(origin + "/health/ready", timeout=5).status == 200
                    break
                except (URLError, ConnectionError):
                    time.sleep(0.1)
            else:
                raise RuntimeError("Structured publication fixture did not become ready")
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(
                    executable_path=os.environ.get("WORLDNEWS_CHROMIUM", "/usr/bin/chromium"),
                    headless=True, args=["--no-sandbox"])
                context = browser.new_context(viewport={"width": 1100, "height": 900})
                external = []
                def guard(route):
                    if route.request.url.startswith(origin + "/"):
                        route.continue_()
                    else:
                        external.append(route.request.url)
                        route.abort()
                context.route("**/*", guard)
                page = context.new_page()
                for story in ["rich-brief", "rich-standard", "rich-analysis"]:
                    assert page.goto(origin + "/story/" + story).status == 200
                    assert page.locator("article[lang=nl]").count() == 1
                    assert page.locator("img,script,style").count() == 0
                    assert page.locator("#claim-fact-1").is_visible()
                    assert "<img" in page.locator("#claim-fact-1").inner_text()
                    assert page.get_by_role("heading", name="Contexte", exact=True).is_visible()
                    assert page.get_by_role("heading", name="Incertitudes et limites", exact=True).is_visible()
                    if story == "rich-standard":
                        assert page.locator("details").count() == 0
                    else:
                        assert not page.locator("details p").is_visible()
                        page.locator("summary").focus()
                        page.locator("summary").press("Enter")
                        assert page.locator("details p").is_visible()
                    if story == "rich-analysis":
                        page.screenshot(path="/workspace/work/structured-article-preview.png", full_page=True)
                assert page.goto(origin + "/story/legacy-literal").status == 200
                assert page.locator("script,details,#section-context,#section-uncertainty").count() == 0
                assert "# This remains literal prose" in page.locator("article").inner_text()
                assert page.goto(origin + "/story/heading-only").status == 404
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            assert b"synthetic-structure-reader" not in log.read()
            print("PASS: Chromium structured formats/anchors, keyboard progressive summary, inert markup, explicit sections, untouched legacy and private-draft boundary; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
