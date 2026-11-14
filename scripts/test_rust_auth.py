#!/usr/bin/env python3
"""Synthetic local auth/CLI/browser checks; invoked by the disposable DB wrapper."""
import json
import os
import re
import socket
import subprocess
import tempfile
import time
from http.cookiejar import CookieJar
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import HTTPCookieProcessor, ProxyHandler, Request, build_opener

PASSWORD = "synthetic runtime passphrase only"


def exercise(binary, owner_url, reader_url, writer_url):
    cli_env = {"DATABASE_URL": owner_url, "APP_ENV": "development"}
    arguments = [binary, "--create-admin", "runtime-editor", "editor"]
    for expected in [0, 1]:
        command = subprocess.run(arguments, input=(PASSWORD + "\n").encode(), env=cli_env,
                                 capture_output=True, timeout=30)
        assert command.returncode == expected
        assert PASSWORD.encode() not in command.stdout + command.stderr
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    env = dict(DATABASE_URL=reader_url, EDITOR_DATABASE_URL=writer_url,
               NEWS_ADMIN_ENABLED="true", APP_ENV="development", PUBLIC_URL=origin,
               BIND_ADDR=f"127.0.0.1:{port}")
    jar = CookieJar()
    opener = build_opener(ProxyHandler({}), HTTPCookieProcessor(jar))

    def request(path, data=None, add_origin=False):
        headers = {"Origin": origin} if add_origin else {}
        req = Request(origin + path, data=urlencode(data).encode() if data is not None else None,
                      headers=headers)
        try:
            response = opener.open(req, timeout=12)
        except HTTPError as error:
            response = error
        body = response.read().decode()
        assert response.headers.get("X-Request-Id")
        assert response.headers.get("Cache-Control") == "no-store"
        return response, body

    with tempfile.TemporaryFile() as log:
        app = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
        try:
            for _ in range(60):
                try:
                    response, body = request("/health/ready")
                    assert response.status == 200 and json.loads(body)["status"] == "schema_ready"
                    break
                except (URLError, ConnectionError):
                    time.sleep(0.1)
            else:
                raise RuntimeError("Auth test process did not become ready")
            assert request("/admin")[0].status == 401
            credentials = {"username": "runtime-editor", "password": PASSWORD}
            assert request("/admin/login", credentials)[0].status == 403
            response, body = request("/admin/login", credentials, True)
            assert response.status == 200 and response.url == origin + "/admin"
            assert "Rôle : éditeur" in body
            csrf = re.search(r'name="csrf" value="([0-9a-f]{64})"', body).group(1)
            secrets = [PASSWORD, csrf] + [cookie.value for cookie in jar]
            assert len(list(jar)) == 2
            assert request("/admin/logout", {}, True)[0].status == 403
            assert request("/admin")[0].status == 200
            response, _ = request("/admin/logout", {"csrf": csrf}, True)
            assert response.url == origin + "/admin/login"
            assert len(list(jar)) == 0
            assert request("/admin")[0].status == 401

            if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1":
                from playwright.sync_api import sync_playwright
                with sync_playwright() as playwright:
                    browser = playwright.chromium.launch(
                        executable_path=os.environ.get("WORLDNEWS_CHROMIUM", "/usr/bin/chromium"),
                        headless=True, args=["--no-sandbox"])
                    context = browser.new_context()
                    page = context.new_page()
                    requests = []
                    page.on("request", lambda req: requests.append(req.url))
                    page.goto(origin + "/admin/login")
                    page.get_by_label("Identifiant", exact=True).fill("runtime-editor")
                    page.get_by_label("Mot de passe", exact=True).fill(PASSWORD)
                    with page.expect_response(lambda response: response.request.method == "POST"
                                              and response.url == origin + "/admin/login") as submitted:
                        page.get_by_role("button", name="Se connecter", exact=True).click()
                    assert submitted.value.request.headers.get("origin") == origin
                    assert submitted.value.status == 303
                    page.wait_for_url(origin + "/admin")
                    assert page.get_by_text("Rôle : éditeur.", exact=True).is_visible()
                    cookies = context.cookies()
                    assert len(cookies) == 2
                    assert all(cookie["httpOnly"] and cookie["sameSite"] == "Strict"
                               and cookie["path"] == "/" for cookie in cookies)
                    secrets.extend(cookie["value"] for cookie in cookies)
                    page.get_by_role("button", name="Se déconnecter", exact=True).click()
                    page.wait_for_url(origin + "/admin/login")
                    assert not context.cookies()
                    assert page.goto(origin + "/admin").status == 401
                    assert all(url.startswith(origin + "/") for url in requests)
                    browser.close()
                print("PASS: Chromium form sign-in/Origin, HttpOnly/SameSite cookies, CSRF logout; no external requests")

            disabled = subprocess.run([binary, "--disable-admin", "runtime-editor"], env=cli_env,
                                      capture_output=True, timeout=20)
            assert disabled.returncode == 0
            assert request("/admin/login", credentials, True)[0].status == 401
            assert request("/")[0].status == 200
            app.terminate()
            assert app.wait(timeout=10) == 0
            log.seek(0)
            logs = log.read()
            for value in secrets + ["synthetic-auth-reader", "synthetic-auth-writer"]:
                assert value.encode() not in logs
            print("PASS: real bootstrap/duplicate/disable CLI, separated reader/writer auth, form CSRF/logout and private logs")
        finally:
            if app.poll() is None:
                app.kill()
                app.wait()
