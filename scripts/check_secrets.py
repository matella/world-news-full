#!/usr/bin/env python3
"""Offline common-secret check of Git-tracked text, including staged new files.

This is a narrow CI guard, not a comprehensive secret scanner or history audit.
It never prints matching values. Synthetic DB credentials are intentionally used
in isolated tests; production credentials must be injected at runtime.
"""
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PATTERNS = {
    "private key": re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"),
    "GitHub token": re.compile(rb"\b(?:ghp_|github_pat_)[A-Za-z0-9_]{30,}\b"),
    "AWS access key": re.compile(rb"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
}


def main():
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    findings = []
    for relative in filter(None, paths):
        path = ROOT / relative
        if not path.is_file():
            continue
        data = path.read_bytes()
        for name, pattern in PATTERNS.items():
            if pattern.search(data):
                findings.append(f"{relative}: possible {name}")
    if findings:
        raise SystemExit("\n".join(findings))
    print("PASS: no common private-key/provider-token patterns in tracked files")


if __name__ == "__main__":
    main()
