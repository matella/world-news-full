#!/usr/bin/env python3
"""Disposable Postgres 18 recreation/restore drill; never touches existing resources."""
import os
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

DOCKER = ["docker", "--host=unix:///var/run/docker.sock"]


def run(*args, input=None):
    result = subprocess.run(DOCKER + list(args), input=input, capture_output=True)
    if result.returncode:
        raise subprocess.CalledProcessError(result.returncode, DOCKER + list(args),
                                           output=result.stdout, stderr=result.stderr)
    return result.stdout


def main():
    # Each resource is created by this run; no compose down, prune or shared names.
    prefix = "worldnews-drill-" + uuid.uuid4().hex[:12]
    source, restored = prefix + "-source", prefix + "-restore"
    volumes = [prefix + "-data", prefix + "-restored-data"]
    created_containers, created_volumes = set(), []
    image = os.environ.get("WORLDNEWS_TEST_POSTGRES_IMAGE", "postgres:18")

    def start(name, volume):
        created_containers.add(name)
        run("run", "-d", "--name", name, "-e", "POSTGRES_PASSWORD=synthetic-drill-only",
            "--mount", f"type=volume,src={volume},dst=/var/lib/postgresql", image)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            try:
                run("exec", name, "pg_isready", "-U", "postgres")
                # pg_isready can succeed against the temporary bootstrap server.
                value = run("exec", name, "psql", "-U", "postgres", "-Atc", "SELECT 1")
                if value.strip() == b"1":
                    time.sleep(1)
                    run("exec", name, "psql", "-U", "postgres", "-Atc", "SELECT 1")
                    return
            except subprocess.CalledProcessError:
                pass
            time.sleep(0.5)
        raise RuntimeError("Disposable database did not become ready")

    def sql(name, statement):
        return run("exec", name, "psql", "-U", "postgres", "-v", "ON_ERROR_STOP=1", "-Atc", statement).strip()

    try:
        for volume in volumes:
            run("volume", "create", volume)
            created_volumes.append(volume)
        start(source, volumes[0])
        assert sql(source, "SHOW data_directory").startswith(b"/var/lib/postgresql/18/")
        sql(source, "CREATE TABLE recovery_probe(id int PRIMARY KEY, title text); INSERT INTO recovery_probe VALUES (1, 'synthetic archive record')")
        run("rm", "-f", source)
        created_containers.remove(source)
        start(source, volumes[0])
        assert sql(source, "SELECT title FROM recovery_probe WHERE id=1") == b"synthetic archive record"
        with tempfile.TemporaryDirectory(prefix="worldnews-backup-") as tmp:
            backup = Path(tmp) / "synthetic.dump"
            backup.write_bytes(run("exec", source, "pg_dump", "-U", "postgres", "-Fc", "postgres"))
            # Release the first container's writable layer before creating another
            # on small/VFS Docker hosts. The original named volume remains intact.
            run("rm", "-f", source)
            created_containers.remove(source)
            start(restored, volumes[1])
            run("exec", "-i", restored, "pg_restore", "-U", "postgres", "--exit-on-error", "-d", "postgres", input=backup.read_bytes())
        assert sql(restored, "SELECT count(*) FROM recovery_probe") == b"1"
        assert sql(restored, "SELECT title FROM recovery_probe WHERE id=1") == b"synthetic archive record"
        print("PASS: Postgres 18 parent mount, container recreation, dump/restore into fresh volume, integrity")
    finally:
        for name in created_containers:
            subprocess.run(DOCKER + ["rm", "-f", name], capture_output=True)
        for volume in created_volumes:
            subprocess.run(DOCKER + ["volume", "rm", volume], capture_output=True)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        print(error.stderr.decode(errors="replace"))
        raise
