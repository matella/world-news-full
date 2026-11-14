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
                # The same restricted editor can author plain blocks without a client script.
                assert page.goto(origin + "/admin/editor").status == 200
                page.get_by_role("link",name="Écrire un article",exact=True).click()
                form = page.locator('form[action="/admin/editor"]')
                form.get_by_label("Titre",exact=True).fill("Nouvelle note <script>littérale</script>")
                form.locator('[name="kind_0"]').select_option("paragraph")
                prose = "Premier paragraphe <script>reste du texte</script>."
                form.locator('[name="text_0"]').fill(prose)
                form.get_by_role("button",name="Ajouter un bloc sans enregistrer").click()
                form = page.locator('form[action="/admin/editor"]')
                assert form.locator('[name="text_0"]').input_value() == prose
                form.locator('[name="kind_1"]').select_option("claim")
                form.locator('[name="text_1"]').fill("Une affirmation à étayer.")
                form.get_by_role("button",name="Prévisualiser sans enregistrer").click()
                assert page.get_by_role("heading",name="Prévisualisation non enregistrée").is_visible()
                assert page.locator("article script,article img").count() == 0
                assert prose in page.locator("article").inner_text()
                form = page.locator('form[action="/admin/editor"]')
                anchor = form.locator('[name="claim_1"]').input_value()
                assert anchor.startswith("c-")
                form.get_by_role("button",name="Enregistrer une version privée").click()
                assert "/admin/editor/story/" in page.url
                form = page.locator('form[action^="/admin/editor/story/"]')
                assert form.locator('[name="claim_1"]').input_value() == anchor
                form.locator('[name="title"]').fill("Un titre corrigé")
                form.locator('[name="reason"]').fill("Corriger le titre sans changer la rédaction")
                button = form.get_by_role("button",name="Enregistrer une version privée")
                button.focus()
                button.press("Enter")
                page.wait_for_load_state("domcontentloaded")
                form = page.locator('form[action^="/admin/editor/story/"]')
                assert form.locator('[name="text_0"]').input_value() == prose
                assert form.locator('[name="claim_1"]').input_value() == anchor
                assert page.locator("script,img").count() == 0
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.screenshot(path="/workspace/work/authoring-preview.png",full_page=True)
                # Explicit source selection keeps original-method notes private.
                page.get_by_role("link",name="Attribuer les sources et les affirmations").click()
                evidence = page.locator('form[action$="/sources"]')
                evidence.locator('[name="mode"]').select_option("external")
                evidence.get_by_label("Attribution publique",exact=True).fill("Source documentaire synthétique")
                evidence.get_by_label("Limites et incertitudes publiques").fill("Une seule source, origine non vérifiée")
                evidence.get_by_role("button",name="Ajouter une référence sans enregistrer").click()
                evidence = page.locator('form[action$="/sources"]')
                assert evidence.count() == 1, page.locator("main").inner_text()
                assert evidence.get_by_label("Attribution publique",exact=True).input_value() == "Source documentaire synthétique"
                # The fixture has >50 catalog records: reach the actual next page.
                evidence.get_by_role("button",name="Page suivante du catalogue").click()
                evidence = page.locator('form[action$="/sources"]')
                assert evidence.get_by_label("Attribution publique",exact=True).input_value() == "Source documentaire synthétique"
                evidence.locator('[name="source_0"]').select_option("reference")
                evidence.locator('[name="claim_0"]').select_option(anchor)
                evidence.get_by_label("Emplacement précis dans le document").fill("Page de démonstration")
                evidence.get_by_role("button",name="Afficher le début du catalogue").click()
                evidence = page.locator('form[action$="/sources"]')
                assert evidence.locator('[name="source_0"]').input_value() == "reference"
                assert evidence.locator('[name="claim_0"]').input_value() == anchor
                evidence.get_by_role("button",name="Ajouter une méthode sans enregistrer").click()
                evidence = page.locator('form[action$="/sources"]')
                evidence.get_by_label("Attribution publique de la méthode").fill("Observation synthétique déclarée")
                evidence.get_by_label("Justificatif privé").fill("PRIVATE-BROWSER-SOURCE </textarea><script>texte inerte</script>")
                evidence.locator('[name="new_claim_0"]').select_option(anchor)
                evidence.get_by_role("button",name="Conserver cette affirmation sans enregistrer").click()
                evidence = page.locator('form[action$="/sources"]')
                assert evidence.locator('[name="scope_0_0"]').is_checked()
                evidence.get_by_label("Motif privé de la nouvelle révision").fill("Attribuer explicitement les sources du brouillon")
                button = evidence.get_by_role("button",name="Enregistrer les sources dans une version privée")
                button.focus()
                button.press("Enter")
                page.wait_for_load_state("domcontentloaded")
                evidence = page.locator('form[action$="/sources"]')
                assert evidence.locator('[name="origin_0"]').input_value() == "0"
                assert evidence.get_by_label("Justificatif privé").input_value().startswith("PRIVATE-BROWSER-SOURCE")
                assert page.locator("script,img").count() == 0
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.screenshot(path="/workspace/work/source-editor-preview.png",full_page=True)
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            assert b"synthetic-length-reader" not in log.read()
            print("PASS: Chromium private count/range/warnings, keyboard overrides, plain authoring/add/preview/save/title edit, stable claims, explicit sources/private methods, mobile layout and honest anonymous estimates; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
