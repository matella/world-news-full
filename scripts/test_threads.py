#!/usr/bin/env python3
"""Private human thread-curation browser journey on synthetic disposable fixtures."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url, writer_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_thread_reader:")
    assert "@127.0.0.1:" in reader_url and reader_url.endswith("_threads")
    assert writer_url.startswith("postgres://fixture_thread_writer:")
    assert "@127.0.0.1:" in writer_url and writer_url.endswith("_threads")
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
                raise RuntimeError("Thread fixture did not become ready")
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
                page.get_by_label("Identifiant").fill("thread-browser")
                page.get_by_label("Mot de passe").fill("synthetic thread curation passphrase")
                page.get_by_role("button",name="Se connecter",exact=True).click()
                root = "/admin/editor/threads"
                def create(key, title):
                    assert page.goto(origin + root).status == 200
                    form = page.locator('form[action="' + root + '"]')
                    form.get_by_label("Identifiant stable du suivi",exact=True).fill(key)
                    form.get_by_label("Titre du suivi",exact=True).fill(title)
                    form.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("PRIVATE-BROWSER-THREAD identité explicite")
                    form.get_by_role("button",name="Créer le suivi",exact=True).click()
                    assert page.url.endswith(root + "/" + key)
                create("browser-left", "Un sujet <script>littéral</script>")
                left = root + "/browser-left"
                member = left + "/member"
                def attach(story, known=None):
                    assert page.goto(origin + left).status == 200
                    form = page.locator('form[action="' + member + '"]')
                    form.get_by_label("Préparer un article par son identifiant stable",exact=True).fill(story)
                    form.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("PRIVATE-BROWSER-THREAD rattachement délibéré")
                    if known:
                        form.get_by_label("Instant connu avec décalage UTC (ex. 2026-11-14T10:00:00+01:00)",exact=True).fill(known)
                    else:
                        form.get_by_label("Instant d’évolution explicitement inconnu",exact=True).check()
                    version = form.locator('[name="version"]').input_value()
                    form.get_by_role("button",name="Préparer cet article sans rattacher",exact=True).click()
                    form = page.locator('form[action="' + member + '"]')
                    assert form.locator('[name="story"]').input_value() == story
                    assert form.locator('[name="version"]').input_value() == version
                    assert form.get_by_label("Motif privé du changement (une ligne)",exact=True).input_value().startswith("PRIVATE-BROWSER-THREAD")
                    button = form.get_by_role("button",name="Enregistrer le rattachement et l’instant",exact=True)
                    button.focus()
                    button.press("Enter")
                    page.wait_for_load_state("domcontentloaded")
                attach("zz-thread-story-1")
                assert "Inconnu" in page.locator("main").inner_text()
                attach("zz-thread-story-2", "2026-11-14T12:30:00+01:00")
                assert "2026-11-14T11:30:00.000000Z" in page.locator("main").inner_text()
                split = page.locator('form[action="' + left + '/split"]')
                split.get_by_label("Identifiant stable du nouveau suivi",exact=True).fill("browser-cut")
                split.get_by_label("Titre du nouveau suivi",exact=True).fill("Sous-ensemble choisi")
                split.get_by_label("Synthetic zz-thread-story-2 · zz-thread-story-2",exact=True).check()
                split.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("Séparer seulement la deuxième évolution")
                split.get_by_role("button",name="Créer le suivi et déplacer ces rattachements",exact=True).click()
                page.locator('a[href^="/admin/editor/thread-actions/"]').first.click()
                inverse = page.locator('form[action^="/admin/editor/thread-actions/"]')
                assert inverse.locator('[name="fences"]').input_value() == "2"
                inverse.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("Annuler la séparation si les deux états sont inchangés")
                inverse.get_by_role("button",name="Annuler cette action si l’état est inchangé",exact=True).click()
                assert page.goto(origin + left).status == 200
                assert "2 articles, y compris privés" in page.locator("main").inner_text()
                create("browser-right", "Destination explicite")
                assert page.goto(origin + left).status == 200
                preview = page.locator('form[action="' + left + '/merge-preview"]')
                preview.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("Réunir deux identités après examen humain")
                for _ in range(20):
                    if preview.locator('option[value="browser-right"]').count():
                        break
                    preview.get_by_role("button",name="Autres destinations sans fusionner",exact=True).click()
                    preview = page.locator('form[action="' + left + '/merge-preview"]')
                preview.get_by_label("Suivi actif de destination",exact=True).select_option("browser-right")
                preview.get_by_role("button",name="Examiner la fusion avant confirmation",exact=True).click()
                confirm = page.locator('form[action="' + left + '/merge"]')
                assert confirm.locator('[name="target"]').input_value() == "browser-right"
                assert confirm.locator('select[name="target"]').count() == 0
                button = confirm.get_by_role("button",name="Confirmer la fusion des rattachements",exact=True)
                button.focus()
                button.press("Enter")
                page.wait_for_load_state("domcontentloaded")
                assert page.url.endswith(root + "/browser-right")
                assert page.goto(origin + left).status == 200
                assert page.locator('form[action="' + left + '/edit"]').count() == 0
                assert page.get_by_role("link",name="Identité redirigée : ouvrir la destination browser-right",exact=True).is_visible()
                page.locator('a[href^="/admin/editor/thread-actions/"]').first.click()
                inverse = page.locator('form[action^="/admin/editor/thread-actions/"]')
                inverse.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("Rétablir les deux suivis sans changer les articles")
                inverse.get_by_role("button",name="Annuler cette action si l’état est inchangé",exact=True).click()
                assert page.goto(origin + left).status == 200
                remove = page.locator('form[action="' + left + '/remove"]')
                remove.get_by_label("Article à retirer",exact=True).select_option("zz-thread-story-2")
                remove.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("Retirer un rattachement seulement")
                remove.get_by_role("button",name="Retirer le rattachement",exact=True).click()
                edit = page.locator('form[action="' + left + '/edit"]')
                edit.get_by_label("Titre du suivi",exact=True).fill("Sujet clos explicitement")
                edit.get_by_label("État",exact=True).select_option("closed")
                edit.get_by_label("Motif privé du changement (une ligne)",exact=True).fill("Clore ce suivi sans effacer son historique")
                edit.get_by_role("button",name="Enregistrer le titre et l’état",exact=True).click()
                assert "État : Clos" in page.locator("main").inner_text()
                assert page.locator("script,img").count() == 0
                page.set_viewport_size({"width":390,"height":844})
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                page.screenshot(path="/workspace/work/thread-curation-preview.png",full_page=True)
                page.get_by_role("button",name="Se déconnecter",exact=True).click()
                assert page.goto(origin + "/admin/login").status == 200
                page.get_by_label("Identifiant").fill("thread-editor")
                page.get_by_label("Mot de passe").fill("synthetic thread curation passphrase")
                page.get_by_role("button",name="Se connecter",exact=True).click()
                assert page.goto(origin + left).status == 200
                assert page.locator('form[action^="' + root + '"]').count() == 0
                assert "Historique audité" in page.locator("main").inner_text()
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            output = log.read()
            assert b"PRIVATE-BROWSER-THREAD" not in output
            assert b"synthetic-thread-writer" not in output
            print("PASS: Chromium private thread/create/lookup/known-unknown dates, split/exact undo, merge confirmation/alias/inverse, removal/closure, keyboard, editor role and mobile layout; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
