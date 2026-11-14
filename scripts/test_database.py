#!/usr/bin/env python3
"""Run Rust DB integration tests against one uniquely named disposable Postgres."""
import os
import json
import subprocess
import socket
import tempfile
import time
import uuid
from pathlib import Path
from urllib.error import URLError
from urllib.request import build_opener, ProxyHandler

from test_persistence import DOCKER, run

ROOT = Path(__file__).resolve().parents[1]


def container_logs(name):
    result=subprocess.run(DOCKER+["logs","--tail","40",name],capture_output=True)
    return (result.stdout+result.stderr)[-8000:].decode(errors="replace")


def main():
    suffix = uuid.uuid4().hex[:12]
    name = "worldnews-schema-" + suffix
    database = "worldnews_disposable_" + suffix
    try:
        run("run", "-d", "--name", name, "-p", "127.0.0.1::5432", "-e",
            "POSTGRES_PASSWORD=synthetic-schema-only", "-e", "POSTGRES_DB=" + database,
            os.environ.get("WORLDNEWS_TEST_POSTGRES_IMAGE", "postgres:18-alpine"))
        for attempt in range(120):
            try:
                run("exec", name, "psql", "-U", "postgres", "-d", database, "-Atc", "SELECT 1")
                time.sleep(1)
                run("exec", name, "psql", "-U", "postgres", "-d", database, "-Atc", "SELECT 1")
                break
            except subprocess.CalledProcessError:
                state=run("inspect",name,"--format","{{.State.Status}}").decode().strip()
                if state == "exited":
                    raise RuntimeError("Disposable Postgres exited: " + container_logs(name))
                time.sleep(0.5)
        else:
            raise RuntimeError("Disposable Postgres did not become ready: " + container_logs(name))
        port = run("port", name, "5432").decode().strip().rsplit(":", 1)[1]
        env = dict(os.environ,
                   WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{database}",
                   WORLDNEWS_TEST_ALLOW_RESET="1")
        subprocess.run(["cargo", "test", "-p", "world-news", "--locked", "--test", "database", "--", "--ignored", "--test-threads=1"],
                       cwd=ROOT, env=env, check=True)
        if (ROOT/"backend/news/tests/legacy_import.rs").is_file():
            legacy_database=database+"_legacy"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+legacy_database)
            legacy_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{legacy_database}")
            subprocess.run(["cargo","test","-p","world-news","--locked","--test","legacy_import","--","--ignored","--test-threads=1"],cwd=ROOT,env=legacy_env,check=True)
        binary = ROOT / "target/debug/world-news"
        if not binary.is_file():
            raise RuntimeError("Build the binary before the database integration wrapper")
        app_env = dict(DATABASE_URL=env["WORLDNEWS_TEST_DATABASE_URL"], APP_ENV="development")
        command = subprocess.run([binary, "--migrate"], env=app_env, capture_output=True, timeout=30)
        assert command.returncode == 0, "Migration CLI failed"
        assert b"synthetic-schema-only" not in command.stdout + command.stderr
        if (ROOT/"backend/news/tests/legacy_import.rs").is_file():
            fixture=ROOT/"fixtures/legacy-stories.json"
            dry=subprocess.run([binary,"--import-legacy",fixture],env={},capture_output=True,timeout=20)
            assert dry.returncode==2  # Explicit attention required for the intentionally invalid row.
            report=json.loads(dry.stdout)
            assert report["dry_run"] and not report["database_checked"] and report["validated_rows"]==3
            assert "Invented river project" not in dry.stdout.decode()
            apply_env=dict(DATABASE_URL=legacy_env["WORLDNEWS_TEST_DATABASE_URL"],APP_ENV="development")
            applied=subprocess.run([binary,"--import-legacy",fixture,"--apply"],env=apply_env,capture_output=True,timeout=30)
            assert applied.returncode==2
            assert json.loads(applied.stdout)["already_imported"]==3
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            web_port = sock.getsockname()[1]
        app_env.update(BIND_ADDR=f"127.0.0.1:{web_port}",
                       DATABASE_URL=f"postgres://fixture_public:synthetic-public-only@127.0.0.1:{port}/{database}")
        with tempfile.TemporaryFile() as log:
            app = subprocess.Popen([binary], env=app_env, stdout=log, stderr=log)
            try:
                opener = build_opener(ProxyHandler({}))
                for attempt in range(60):
                    try:
                        response = opener.open(f"http://127.0.0.1:{web_port}/health/ready", timeout=5)
                        assert response.status == 200
                        break
                    except (URLError, ConnectionError):
                        time.sleep(0.1)
                else:
                    raise RuntimeError("Migrated-schema readiness did not succeed with reader role")
            finally:
                app.terminate()
                app.wait(timeout=10)
        print("PASS: isolated schema upgrade, immutability, snapshot and reader-boundary integration")
    finally:
        subprocess.run(DOCKER + ["rm", "-f", "-v", name], capture_output=True)


if __name__ == "__main__":
    main()
