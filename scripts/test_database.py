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
from urllib.error import HTTPError, URLError
from urllib.request import build_opener, ProxyHandler

from test_persistence import DOCKER, run

ROOT = Path(__file__).resolve().parents[1]


def cargo_suite(suite, env):
    try:
        subprocess.run(["cargo", "test", "-p", "world-news", "--locked", "--test", suite,
                        "--", "--ignored", "--test-threads=1"], cwd=ROOT, env=env, check=True)
    finally:
        # Optional cleanup of regenerable test binaries on small Docker/VFS hosts.
        # Source, dependencies, the application binary and assertions are untouched.
        if os.environ.get("WORLDNEWS_TEST_RECLAIM_BINARIES") == "1":
            for path in (ROOT/"target/debug/deps").glob(suite.replace("-", "_")+"-*"):
                if path.is_file() and not path.suffix and os.access(path, os.X_OK):
                    path.unlink()


def container_logs(name):
    result=subprocess.run(DOCKER+["logs","--tail","40",name],capture_output=True)
    return (result.stdout+result.stderr)[-8000:].decode(errors="replace")


def main():
    suffix = uuid.uuid4().hex[:12]
    name = "worldnews-schema-" + suffix
    database = "worldnews_disposable_" + suffix
    # Optional memory-backed fixture storage for small/VFS Docker workspaces.
    # This suite checks schema/services, not the separate persistence drill.
    storage = ["--tmpfs", "/var/lib/postgresql:rw,size=1g"] if os.environ.get("WORLDNEWS_TEST_PG_TMPFS") == "1" else []
    try:
        run("run", "-d", "--name", name, *storage, "-p", "127.0.0.1::5432", "-e",
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
        cargo_suite("database", env)
        if (ROOT/"backend/news/tests/legacy_import.rs").is_file():
            legacy_database=database+"_legacy"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+legacy_database)
            legacy_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{legacy_database}")
            cargo_suite("legacy_import", legacy_env)
        if (ROOT/"backend/news/tests/http.rs").is_file():
            http_database=database+"_http"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+http_database)
            http_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{http_database}")
            cargo_suite("http", http_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+http_database)
        if (ROOT/"backend/news/tests/auth.rs").is_file():
            auth_database=database+"_auth"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+auth_database)
            auth_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{auth_database}")
            cargo_suite("auth", auth_env)
        if (ROOT/"backend/news/tests/editorial.rs").is_file():
            editorial_database=database+"_editorial"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+editorial_database)
            editorial_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{editorial_database}")
            cargo_suite("editorial", editorial_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+editorial_database)
        if (ROOT/"backend/news/tests/registry.rs").is_file():
            registry_database=database+"_registry"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+registry_database)
            registry_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{registry_database}")
            cargo_suite("registry", registry_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+registry_database)
        if (ROOT/"backend/news/tests/feed_poll.rs").is_file():
            feed_database=database+"_feed"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+feed_database)
            feed_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{feed_database}")
            cargo_suite("feed_poll", feed_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+feed_database)
        if (ROOT/"backend/news/tests/normalize.rs").is_file():
            normalize_database=database+"_normalize"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+normalize_database)
            normalize_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{normalize_database}")
            cargo_suite("normalize", normalize_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+normalize_database)
        if (ROOT/"backend/news/tests/intake.rs").is_file():
            intake_database=database+"_intake"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+intake_database)
            intake_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{intake_database}")
            cargo_suite("intake", intake_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+intake_database)
        if (ROOT/"backend/news/tests/jobs.rs").is_file():
            jobs_database=database+"_jobs"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+jobs_database)
            jobs_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{jobs_database}")
            cargo_suite("jobs", jobs_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+jobs_database)
        if (ROOT/"backend/news/tests/feed_jobs.rs").is_file():
            feed_jobs_database=database+"_feed_jobs"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+feed_jobs_database)
            feed_jobs_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{feed_jobs_database}")
            cargo_suite("feed_jobs", feed_jobs_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+feed_jobs_database)
        if (ROOT/"backend/news/tests/extraction_jobs.rs").is_file():
            extraction_jobs_database=database+"_extraction_jobs"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+extraction_jobs_database)
            extraction_jobs_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{extraction_jobs_database}")
            cargo_suite("extraction_jobs", extraction_jobs_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+extraction_jobs_database)
        if (ROOT/"backend/news/tests/worker.rs").is_file():
            worker_database=database+"_worker"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+worker_database)
            worker_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{worker_database}")
            cargo_suite("worker", worker_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+worker_database)
        if (ROOT/"backend/news/tests/origins.rs").is_file():
            origins_database=database+"_origins"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+origins_database)
            origins_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{origins_database}")
            cargo_suite("origins", origins_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+origins_database)
        if (ROOT/"backend/news/tests/length.rs").is_file():
            length_database=database+"_length"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+length_database)
            length_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{length_database}")
            cargo_suite("length", length_env)
            if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1":
                from test_length import exercise as exercise_length
                exercise_length(ROOT/"target/debug/world-news",f"postgres://fixture_length_reader:synthetic-length-reader@127.0.0.1:{port}/{length_database}",f"postgres://fixture_length_writer:synthetic-length-writer@127.0.0.1:{port}/{length_database}")
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+length_database)
        if (ROOT/"backend/news/tests/citations.rs").is_file():
            citations_database=database+"_citations"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+citations_database)
            citations_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{citations_database}")
            cargo_suite("citations", citations_env)
            if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1":
                from test_citations import exercise as exercise_citations
                exercise_citations(ROOT/"target/debug/world-news",f"postgres://fixture_citation_reader:synthetic-citation-reader@127.0.0.1:{port}/{citations_database}")
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+citations_database)
        if (ROOT/"backend/news/tests/structured_content.rs").is_file():
            structured_database=database+"_structured"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+structured_database)
            structured_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{structured_database}")
            cargo_suite("structured_content", structured_env)
            if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1":
                from test_structured_content import exercise as exercise_content
                exercise_content(ROOT/"target/debug/world-news",f"postgres://fixture_structure_reader:synthetic-structure-reader@127.0.0.1:{port}/{structured_database}")
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+structured_database)
        if (ROOT/"backend/news/tests/threads.rs").is_file():
            threads_database=database+"_threads"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+threads_database)
            threads_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{threads_database}")
            cargo_suite("threads", threads_env)
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","DROP DATABASE "+threads_database)
        if (ROOT/"backend/news/tests/admin_ui.rs").is_file():
            admin_database=database+"_admin_ui"
            run("exec",name,"psql","-U","postgres","-d","postgres","-v","ON_ERROR_STOP=1","-c","CREATE DATABASE "+admin_database)
            admin_env=dict(env,WORLDNEWS_TEST_DATABASE_URL=f"postgres://postgres:synthetic-schema-only@127.0.0.1:{port}/{admin_database}")
            cargo_suite("admin_ui", admin_env)
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
                if (ROOT/"backend/news/tests/http.rs").is_file():
                    for path,expected in [("/",200),("/archive",200),("/story/b",404),("/edition/2026-02-30",400),("/story/new-draft",503),("/recherche?q=synthetic-sensitive-query",501)]:
                        try:
                            response=opener.open(f"http://127.0.0.1:{web_port}"+path,timeout=12)
                        except HTTPError as error:
                            response=error
                        assert response.status==expected, (path,response.status)
                        assert response.headers.get("X-Request-Id")
                        assert response.headers.get("Cache-Control")=="no-store"
                        assert b"synthetic-public-only" not in response.read()
                    log.seek(0)
                    logs=log.read()
                    assert b"synthetic-sensitive-query" not in logs
                    assert b"synthetic-public-only" not in logs
            finally:
                app.terminate()
                app.wait(timeout=10)
        print("PASS: isolated schema upgrade, immutability, snapshot and reader-boundary integration")
        if (ROOT/"backend/news/tests/auth.rs").is_file():
            from test_rust_auth import exercise
            exercise(binary,auth_env["WORLDNEWS_TEST_DATABASE_URL"],
                     f"postgres://fixture_auth_reader:synthetic-auth-reader@127.0.0.1:{port}/{auth_database}",
                     f"postgres://fixture_auth_writer:synthetic-auth-writer@127.0.0.1:{port}/{auth_database}")
        if os.environ.get("WORLDNEWS_BROWSER_CHECK") == "1" and (ROOT/"backend/news/tests/admin_ui.rs").is_file():
            from test_admin_ui import exercise as exercise_pipeline
            exercise_pipeline(binary,
                     f"postgres://fixture_ops_reader:synthetic-ops-reader@127.0.0.1:{port}/{admin_database}",
                     f"postgres://fixture_ops_writer:synthetic-ops-writer@127.0.0.1:{port}/{admin_database}")
    finally:
        subprocess.run(DOCKER + ["rm", "-f", "-v", name], capture_output=True)


if __name__ == "__main__":
    main()
