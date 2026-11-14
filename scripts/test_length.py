#!/usr/bin/env python3
"""Anonymous browser check of synthetic length fixtures only."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url, writer_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_length_reader:")
    assert "@127.0.0.1:" in reader_url and reader_url.endswith("_length")
    assert writer_url.startswith("postgres://fixture_length_writer:")
    assert "@127.0.0.1:" in writer_url and writer_url.endswith("_length")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    env = dict(DATABASE_URL=reader_url, APP_ENV="development", PUBLIC_URL=origin,
               BIND_ADDR=f"127.0.0.1:{port}", EDITOR_DATABASE_URL=writer_url, NEWS_ADMIN_ENABLED="true")
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
                raise RuntimeError("Length fixture did not become ready")
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
                assert page.goto(origin + "/admin/login").status == 200
                page.get_by_label("Identifiant").fill("length-editor")
                page.get_by_label("Mot de passe").fill("synthetic length verification passphrase")
                page.get_by_role("button",name="Se connecter",exact=True).click()
                assert page.goto(origin + "/admin/drafts/browser-length").status == 200
                assert "79 mots dans le corps" in page.locator("article").inner_text()
                assert "80–150 mots" in page.locator("article").inner_text()
                before = page.locator("article > p").all_inner_texts()
                form = page.locator('form[action$="/length"]')
                public_reason = "Un avis court et complet ; aucun ajout de remplissage."
                form.locator('[name="override_reason"]').fill(public_reason)
                form.locator('[name="reason"]').fill("Évaluation explicite de la notice courte")
                button = form.get_by_role("button",name="Créer une version à relire")
                button.focus()
                button.press("Enter")
                page.wait_for_load_state("domcontentloaded")
                assert public_reason in page.locator("article").inner_text()
                assert page.locator("article > p").all_inner_texts() == before
                assert page.locator('form[action$="/publish"]').count() == 0
                assert "PRIVATE-LENGTH-EVIDENCE" not in page.content()
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.screenshot(path="/workspace/work/length-preview.png",full_page=True)
                assert page.goto(origin + "/story/short").status == 200
                public_text = page.locator("article").inner_text()
                assert "79 mots dans le corps" in public_text
                assert "Temps de lecture estimé : environ 1 min" in public_text
                assert "Repère actuel" not in public_text
                assert "motiver une dérogation" not in public_text
                assert page.goto(origin + "/story/unsupported").status == 200
                assert "Temps de lecture non estimé" in page.locator("article").inner_text()
                assert "environ 1 min" not in page.locator("article").inner_text()
                assert page.goto(origin + "/story/punctuation").status == 404
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            assert b"synthetic-length-reader" not in log.read()
            print("PASS: Chromium private count/range/warnings, keyboard reasoned override without prose changes, mobile layout, anonymous reading estimates and unsupported-language honesty; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
