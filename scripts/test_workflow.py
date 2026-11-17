#!/usr/bin/env python3
"""Synthetic author/review/publish/correct/withdraw workflow; no provider or live data."""
import os
import socket
import subprocess
import tempfile
import time
from urllib.error import URLError
from urllib.parse import urlsplit
from urllib.request import ProxyHandler, build_opener


def exercise(binary, reader_url, writer_url):
    from playwright.sync_api import sync_playwright
    assert reader_url.startswith("postgres://fixture_workflow_reader:")
    assert "@127.0.0.1:" in reader_url and reader_url.endswith("_workflow")
    assert writer_url.startswith("postgres://fixture_workflow_writer:")
    assert "@127.0.0.1:" in writer_url and writer_url.endswith("_workflow")
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
                raise RuntimeError("Workflow fixture did not become ready")
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(executable_path=os.environ.get("WORLDNEWS_CHROMIUM", "/usr/bin/chromium"),headless=True,args=["--no-sandbox"])
                external = []
                def context():
                    ctx = browser.new_context(viewport={"width":1100,"height":900})
                    def guard(route):
                        if route.request.url.startswith(origin + "/"):
                            route.continue_()
                        else:
                            external.append(route.request.url)
                            route.abort()
                    ctx.route("**/*", guard)
                    return ctx
                author = context().new_page()
                reviewer = context().new_page()
                anonymous = context().new_page()
                def login(page,user):
                    assert page.goto(origin + "/admin/login").status == 200
                    page.get_by_label("Identifiant").fill(user)
                    page.get_by_label("Mot de passe").fill("synthetic complete workflow passphrase")
                    page.get_by_role("button",name="Se connecter",exact=True).click()
                login(author,"workflow-editor")
                login(reviewer,"workflow-browser")
                assert author.goto(origin + "/admin/editor/new").status == 200
                body = author.locator('form[action="/admin/editor"]')
                body.get_by_label("Titre",exact=True).fill("Une publication <script>littérale</script>")
                body.locator('[name="kind_0"]').select_option("claim")
                body.locator('[name="text_0"]').fill(" ".join(["mot"]*80))
                body.get_by_role("button",name="Enregistrer une version privée",exact=True).click()
                edit = urlsplit(author.url).path
                story = edit.rsplit("/",1)[1]
                public = "/story/" + story
                workflow = edit + "/workflow"
                source_path = edit + "/sources"
                def evidence(note):
                    assert author.goto(origin + edit).status == 200
                    anchor = author.locator('form[action="' + edit + '"] [name="claim_0"]').input_value()
                    assert author.goto(origin + source_path).status == 200
                    form = author.locator('form[action="' + source_path + '"]')
                    if form.locator('[name="methods"]').input_value() == "0":
                        form.get_by_role("button",name="Ajouter une méthode sans enregistrer",exact=True).click()
                        form = author.locator('form[action="' + source_path + '"]')
                    form.locator('[name="mode"]').select_option("original")
                    form.get_by_label("Attribution publique",exact=True).fill("Observation humaine synthétique déclarée")
                    form.get_by_label("Limites et incertitudes publiques",exact=True).fill("Un seul observateur ; exemple de démonstration sans certification d’indépendance")
                    form.get_by_label("Attribution publique de la méthode",exact=True).fill("Observation synthétique documentée")
                    form.get_by_label("Justificatif privé",exact=True).fill(note)
                    if form.locator('[name="scope_count_0"]').input_value() == "0":
                        form.locator('[name="new_claim_0"]').select_option(anchor)
                    form.get_by_label("Motif privé de la nouvelle révision",exact=True).fill("Confirmer explicitement les justificatifs de cette version")
                    form.get_by_role("button",name="Enregistrer les sources dans une version privée",exact=True).click()
                evidence("PRIVATE-BROWSER-WORKFLOW-PROOF initial observation")
                detail = "/admin/drafts/" + story
                assert author.goto(origin + detail).status == 200
                classify = author.locator('form[action="' + detail + '/classification"]')
                classify.locator('[name="topic:science"]').check()
                classify.locator('[name="geography:world"]').check()
                classify.get_by_label("Motif du classement",exact=True).fill("Classement humain distinct de la langue")
                classify.get_by_role("button",name="Enregistrer le classement dans une nouvelle version",exact=True).click()
                def review_and_publish(reason):
                    assert author.goto(origin + workflow).status == 200
                    assert author.locator('form[action$="/publish"]').count() == 0
                    author.get_by_role("button",name="Demander une relecture",exact=True).click()
                    assert reviewer.goto(origin + workflow).status == 200
                    review = reviewer.locator('form[action$="/review"]')
                    review.get_by_label("Décision de relecture",exact=True).select_option("approved")
                    review.get_by_label("Note de relecture privée, obligatoire pour demander des modifications",exact=True).fill("PRIVATE-BROWSER-WORKFLOW-REVIEW sources et format examinés")
                    review.get_by_role("button",name="Enregistrer la décision de relecture",exact=True).click()
                    publish = reviewer.locator('form[action$="/publish"]')
                    publish.get_by_label("Motif public de publication ou de correction",exact=True).fill(reason)
                    button = publish.get_by_role("button",name="Publier cette version approuvée",exact=True)
                    button.focus()
                    button.press("Enter")
                    reviewer.wait_for_load_state("domcontentloaded")
                review_and_publish("PUBLIC-BROWSER-WORKFLOW-INITIAL")
                assert anonymous.goto(origin + public).status == 200
                initial = anonymous.locator("main").inner_text()
                assert "PUBLIC-BROWSER-WORKFLOW-INITIAL" in initial
                assert "PRIVATE-BROWSER-WORKFLOW" not in initial
                assert anonymous.locator("script,img").count() == 0
                assert author.goto(origin + edit).status == 200
                body = author.locator('form[action="' + edit + '"]')
                body.get_by_label("Titre",exact=True).fill("Une publication corrigée")
                body.locator('[name="text_0"]').fill(" ".join(["corrigé"]*80))
                body.get_by_label("Motif privé de cette révision",exact=True).fill("PRIVATE-BROWSER-WORKFLOW-CHANGE correction de démonstration")
                body.get_by_role("button",name="Enregistrer une version privée",exact=True).click()
                assert anonymous.goto(origin + public).status == 200
                assert "Une publication corrigée" not in anonymous.locator("main").inner_text()
                evidence("PRIVATE-BROWSER-WORKFLOW-PROOF reconfirmed correction")
                review_and_publish("PUBLIC-BROWSER-WORKFLOW-CORRECTION")
                assert anonymous.goto(origin + public).status == 200
                corrected = anonymous.locator("main").inner_text()
                assert "Une publication corrigée" in corrected
                assert "PUBLIC-BROWSER-WORKFLOW-CORRECTION" in corrected
                assert "PRIVATE-BROWSER-WORKFLOW" not in corrected
                assert reviewer.goto(origin + workflow).status == 200
                withdraw = reviewer.locator('form[action$="/withdraw"]')
                withdraw.get_by_label("Motif public du retrait",exact=True).fill("PUBLIC-BROWSER-WORKFLOW-WITHDRAW")
                withdraw.get_by_role("button",name="Retirer l’article avec ce motif public",exact=True).click()
                assert anonymous.goto(origin + public).status == 410
                withdrawn = anonymous.locator("main").inner_text()
                assert "PUBLIC-BROWSER-WORKFLOW-WITHDRAW" in withdrawn
                assert "PRIVATE-BROWSER-WORKFLOW" not in withdrawn
                assert anonymous.locator("article").count() == 0
                assert reviewer.locator('form[action$="/publish"],form[action$="/review"],form[action$="/submit"],form[action$="/withdraw"]').count() == 0
                reviewer.set_viewport_size({"width":390,"height":844})
                anonymous.set_viewport_size({"width":390,"height":844})
                assert reviewer.evaluate("document.documentElement.scrollWidth <= innerWidth")
                assert anonymous.evaluate("document.documentElement.scrollWidth <= innerWidth")
                reviewer.screenshot(path="/workspace/work/editorial-workflow-preview.png",full_page=True)
                assert not external, external
                browser.close()
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            output = log.read()
            assert b"PRIVATE-BROWSER-WORKFLOW" not in output
            assert b"synthetic-workflow-writer" not in output
            print("PASS: Chromium author/source/classification/review/publish/correct/withdraw, distinct roles, keyboard save, anonymous public reasons/410, private-note exclusion and mobile layout; no external requests")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
