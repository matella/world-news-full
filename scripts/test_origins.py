#!/usr/bin/env python3
"""Publisher-origin evidence browser journey on a disposable synthetic database."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url, writer_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_origin_reader:")
    assert "@127.0.0.1:" in reader_url and reader_url.endswith("_origins")
    assert writer_url.startswith("postgres://fixture_origin_writer:")
    assert "@127.0.0.1:" in writer_url and writer_url.endswith("_origins")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    env = dict(DATABASE_URL=reader_url, APP_ENV="development", PUBLIC_URL=origin,
               BIND_ADDR=f"127.0.0.1:{port}", EDITOR_DATABASE_URL=writer_url,
               NEWS_ADMIN_ENABLED="true")
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
                raise RuntimeError("Origin fixture did not become ready")
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(
                    executable_path=os.environ.get("WORLDNEWS_CHROMIUM", "/usr/bin/chromium"),
                    headless=True, args=["--no-sandbox"])
                context = browser.new_context(viewport={"width":1100,"height":900})
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
                page.get_by_label("Identifiant").fill("origin-browser")
                page.get_by_label("Mot de passe").fill("synthetic origin verification passphrase")
                page.get_by_role("button",name="Se connecter",exact=True).click()
                # Configure an identity explicitly; this is separate from article proof.
                registry_root = "/admin/editor/origins"
                assert page.goto(origin + registry_root).status == 200
                registry = page.locator('form[action="/admin/editor/origins"]')
                registry.get_by_label("Identifiant stable de l’origine",exact=True).fill("zz-browser-origin")
                registry.get_by_label("Nom de l’origine",exact=True).fill("Identité de démonstration <script>littérale</script>")
                registry.get_by_label("Source associée (identifiant facultatif)",exact=True).fill("outlet-b")
                old_note = "\nPRIVATE-BROWSER-REGISTRY-OLD </textarea><script>texte</script>"
                registry.get_by_label("Note privée du registre",exact=True).fill(old_note)
                registry.get_by_label("Disponible pour de nouvelles assertions",exact=True).check()
                button = registry.get_by_role("button",name="Créer l’origine",exact=True)
                button.focus()
                button.press("Enter")
                page.wait_for_load_state("domcontentloaded")
                registry_path = registry_root + "/zz-browser-origin"
                assert page.url.endswith(registry_path)
                registry = page.locator('form[action="' + registry_path + '"]')
                assert registry.get_by_label("Note privée du registre",exact=True).input_value() == old_note
                assert registry.get_by_label("Nom de l’origine",exact=True).get_attribute("readonly") is not None
                registry.get_by_label("Disponible pour de nouvelles assertions",exact=True).uncheck()
                registry.get_by_label("Note privée du registre",exact=True).fill("\nPRIVATE-BROWSER-REGISTRY-NEW")
                registry.get_by_role("button",name="Enregistrer la configuration",exact=True).click()
                registry = page.locator('form[action="' + registry_path + '"]')
                assert registry.locator('[name="version"]').input_value() == "1"
                assert not registry.get_by_label("Disponible pour de nouvelles assertions",exact=True).is_checked()
                assert "PRIVATE-BROWSER-REGISTRY-OLD" in page.locator("main").inner_text()
                assert "PRIVATE-BROWSER-REGISTRY-NEW" in page.locator("main").inner_text()
                assert page.locator("script,img").count() == 0
                path = "/admin/editor/source/ui-origin/origins"
                assert page.goto(origin + path).status == 200
                form = page.locator('form[action$="/origins"]')
                assert form.get_by_label("Justificatif privé de cette nouvelle assertion").input_value() == ""
                form.get_by_label("État de l’origine").select_option("documented")
                form.get_by_label("UI origin 00",exact=True).check()
                form.get_by_label("Justification publique de l’attribution").fill("Attribution documentée synthétique")
                note = "PRIVATE-BROWSER-ORIGIN </textarea><script>reste du texte</script>"
                form.get_by_label("Justificatif privé de cette nouvelle assertion").fill(note)
                form.get_by_role("button",name="Autres origines sans enregistrer").click()
                form = page.locator('form[action$="/origins"]')
                assert form.get_by_label("UI origin 00",exact=True).is_checked()
                assert form.get_by_label("Justificatif privé de cette nouvelle assertion").input_value() == note
                form.get_by_label("UI origin 24",exact=True).check()
                button = form.get_by_role("button",name="Enregistrer la preuve d’origine")
                button.focus()
                button.press("Enter")
                page.wait_for_load_state("domcontentloaded")
                form = page.locator('form[action$="/origins"]')
                assert form.locator('[name="version"]').input_value() == "2"
                assert form.get_by_label("Justificatif privé de cette nouvelle assertion").input_value() == ""
                page.get_by_role("link",name="Version 2 · Origine documentée").click()
                assert note in page.locator("main").inner_text()
                assert "UI origin 24" in page.locator("main").inner_text()
                assert page.locator("script,img").count() == 0
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                assert page.goto(origin + path).status == 200
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.screenshot(path="/workspace/work/origin-controls-preview.png",full_page=True)
                page.get_by_role("button",name="Se déconnecter",exact=True).click()
                assert page.goto(origin + "/admin/login").status == 200
                page.get_by_label("Identifiant").fill("origin-editor")
                page.get_by_label("Mot de passe").fill("synthetic origin verification passphrase")
                page.get_by_role("button",name="Se connecter",exact=True).click()
                assert page.goto(origin + path).status == 200
                assert page.get_by_role("button",name="Enregistrer la preuve d’origine").count() == 0
                assert "Seul un responsable de publication" in page.locator("main").inner_text()
                assert page.goto(origin + registry_path).status == 200
                assert page.get_by_role("button",name="Enregistrer la configuration",exact=True).count() == 0
                assert "PRIVATE-BROWSER-REGISTRY-OLD" in page.locator("main").inner_text()
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            output = log.read()
            assert b"PRIVATE-BROWSER-ORIGIN" not in output
            assert b"PRIVATE-BROWSER-REGISTRY" not in output
            assert b"synthetic-origin-writer" not in output
            print("PASS: Chromium origin registry/create/retire/audit, origin choices/pagination, fresh human justification, keyboard sealed save/history, inert private notes, editor role and mobile layout; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
