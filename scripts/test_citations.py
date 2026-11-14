#!/usr/bin/env python3
"""Anonymous browser check of synthetic citation fixtures only."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_citation_reader:")
    assert "@127.0.0.1:" in reader_url and reader_url.endswith("_citations")
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
                raise RuntimeError("Citation fixture did not become ready")
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
                assert page.goto(origin + "/story/sourced").status == 200
                text = page.locator("article").inner_text()
                assert "2 document(s) distinct(s)" in text
                assert "2 média(s) déclaré(s)" in text
                assert "1 origine(s) de reportage documentée(s)" in text
                assert "1 document(s) d’origine inconnue" in text
                assert "Positions contradictoires attribuées" in text
                assert "Attributed outlet A position" in text
                assert "Synthetic named observation" in text
                assert "PRIVATE-" not in page.content()
                assert page.locator("script,img,style").count() == 0
                reference = page.locator("#claim-one sup a")
                reference.focus()
                reference.press("Enter")
                assert page.url.endswith("#reference-1")
                assert page.locator("#reference-1").is_visible()
                assert "Paragraph 2 <script>plain locator</script>" in page.locator("#reference-1").inner_text()
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.screenshot(path="/workspace/work/citations-preview.png",full_page=True)
                assert page.goto(origin + "/story/single").status == 200
                assert "1 document(s) distinct(s)" in page.locator("article").inner_text()
                assert page.goto(origin + "/story/historical").status == 200
                assert "Archive historique non vérifiée" in page.locator("article").inner_text()
                assert page.goto(origin + "/story/fake-history").status == 404
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            assert b"synthetic-citation-reader" not in log.read()
            print("PASS: Chromium citation counts/definitions, keyboard footnotes, attributed disagreements, private-evidence exclusion, mobile layout, single-source and historical labels; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
