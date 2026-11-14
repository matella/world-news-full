#!/usr/bin/env python3
"""Local Chromium pipeline workflow; only disposable test roles and fixtures."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url, writer_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_ops_reader:")
    assert writer_url.startswith("postgres://fixture_ops_writer:")
    assert reader_url.endswith("_admin_ui") and writer_url.endswith("_admin_ui")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    env = dict(DATABASE_URL=reader_url, EDITOR_DATABASE_URL=writer_url,
               NEWS_ADMIN_ENABLED="true", APP_ENV="development", PUBLIC_URL=origin,
               BIND_ADDR=f"127.0.0.1:{port}")
    password = "synthetic pipeline administration passphrase"
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
                raise RuntimeError("Pipeline browser fixture did not become ready")
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(
                    executable_path=os.environ.get("WORLDNEWS_CHROMIUM", "/usr/bin/chromium"),
                    headless=True, args=["--no-sandbox"])
                context = browser.new_context(viewport={"width": 1280, "height": 900})
                external = []
                def guard(route):
                    if route.request.url.startswith(origin + "/"):
                        route.continue_()
                    else:
                        external.append(route.request.url)
                        route.abort()
                context.route("**/*", guard)
                page = context.new_page()
                page.goto(origin + "/admin/login")
                page.get_by_label("Identifiant", exact=True).fill("ops-publisher")
                page.get_by_label("Mot de passe", exact=True).fill(password)
                page.get_by_role("button", name="Se connecter", exact=True).click()
                page.wait_for_url(origin + "/admin")
                page.goto(origin + "/admin/sources")
                form = page.locator('form[action="/admin/sources"]')
                form.get_by_label("Nom de la source", exact=True).fill("Browser fixture")
                form.get_by_label("Page d’accueil", exact=True).fill("https://browser.invalid")
                form.get_by_role("button", name="Enregistrer la source", exact=True).click()
                page.wait_for_url(origin + "/admin/sources/*")
                source_page = page.url
                feed_form = page.locator('form[action="/admin/feeds"]')
                feed_form.get_by_label("Adresse RSS ou Atom", exact=True).fill("https://browser.invalid/rss")
                feed_form.get_by_role("button").click()
                page.wait_for_url(origin + "/admin/feeds/*")
                assert page.get_by_label("Adresse RSS ou Atom", exact=True).get_attribute("readonly") is not None
                page.goto(source_page)
                page.get_by_role("link", name="Documents de cette source", exact=True).click()
                form = page.locator('form[action="/admin/articles"]')
                form.get_by_label("Lien original", exact=True).fill("https://browser.invalid/source")
                form.get_by_label("Titre indiqué par la source", exact=True).fill("Browser source title")
                form.locator('[name="language"]').fill("fr")
                form.locator('[name="notes"]').fill("Notes originales de la rédaction de test.")
                form.get_by_role("button").click()
                page.wait_for_url(origin + "/admin/articles/*")
                form = page.locator('form[action$="/draft"]')
                form.get_by_label("Contexte et notes de rédaction", exact=True).fill("Contexte original de test.")
                form.get_by_label("Notre titre original", exact=True).fill("Browser original draft")
                form.get_by_label("Notre texte original", exact=True).fill("Premier paragraphe original.\n\nDeuxième paragraphe original.")
                form.get_by_label("Format", exact=True).select_option("brief")
                form.get_by_role("button").click()
                page.wait_for_url(origin + "/admin/drafts/*")
                assert page.get_by_role("heading", name="Brouillon : Browser original draft", exact=True).is_visible()
                assert page.get_by_text("Premier paragraphe original.", exact=True).is_visible()
                draft_id = page.url.rsplit("/", 1)[1]
                classification = page.locator('form[action$="/classification"]')
                classification.get_by_label("Sujet : Politique", exact=True).check()
                classification.get_by_label("Pertinence géographique : Belgique", exact=True).check()
                classification.get_by_label("Motif du classement", exact=True).fill("Vérification géographique de la rédaction de test.")
                with page.expect_response(lambda response: response.request.method == "POST" and response.url.endswith("/classification")) as submitted:
                    classification.get_by_role("button").click()
                assert submitted.value.status == 303
                page.get_by_text("Rubrique proposée : Belgique. La langue du texte ne détermine pas sa géographie.", exact=True).wait_for()
                response = context.request.get(origin + "/story/" + draft_id)
                assert response.status == 404
                assert "Browser original draft" not in response.text()
                page.screenshot(path="/workspace/work/admin-ui-preview.png", full_page=True)
                page.goto(origin + "/admin/pipeline")
                assert page.get_by_role("heading", name="Suivi des sources et des traitements", exact=True).is_visible()
                page.screenshot(path="/workspace/work/admin-overview-preview.png", full_page=True)
                page.set_viewport_size({"width": 390, "height": 844})
                page.goto(origin + "/admin/feeds")
                assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            logs = log.read()
            for secret in [password, "synthetic-ops-reader", "synthetic-ops-writer", "Browser original draft"]:
                assert secret.encode() not in logs
            print("PASS: Chromium source/manual-link/original-draft/classification forms, private publication boundary, mobile overflow, no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
