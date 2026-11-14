#!/usr/bin/env python3
"""Offline checks for pinned GADS, editorial settings and synthetic import fixtures."""
import hashlib
import json
import re
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    lock = json.loads((ROOT / "gads/install.json").read_text())
    upstream = json.loads((ROOT / "gads/UPSTREAM.json").read_text())
    require(lock["version"] == upstream["version"], "GADS version mismatch")
    require(re.fullmatch(r"[0-9a-f]{40}", upstream["revision"]), "Unpinned GADS")
    for rel, expected in lock["files"].items():
        path = ROOT / rel
        require(path.resolve().is_relative_to(ROOT), f"Invalid managed path: {rel}")
        data = path.read_bytes().replace(b"\r\n", b"\n")
        require(hashlib.sha256(data).hexdigest() == expected, f"GADS drift: {rel}")
    for rel in ("AGENTS.md", "CLAUDE.md", "gads/PROJECT.md"):
        text = (ROOT / rel).read_text()
        for target in re.findall(r"`(gads/[^`\s]+\.md)`", text):
            require((ROOT / target).is_file(), f"Broken policy reference: {target}")

    taxonomy = ROOT / "config/taxonomy.json"
    require(taxonomy.is_file(), "Missing taxonomy policy")
    require(taxonomy.read_bytes() == (ROOT / "frontend/src/lib/taxonomy.generated.json").read_bytes(),
            "Transitional taxonomy snapshot drift: copy config/taxonomy.json to frontend/src/lib/taxonomy.generated.json")
    policy = json.loads((ROOT / "config/editorial.json").read_text())
    require(policy["version"] == 1, "Unsupported editorial config version")
    require(policy["publication_mode"] == "manual", "Automatic publication is not approved")
    require(not policy["automatic_publication_enabled"], "Automatic publication enabled")
    require(not policy["ai_drafting_enabled"], "AI drafting must initially be disabled")
    require(policy["reading_words_per_minute"] > 0, "Invalid reading speed")
    for name, budget in policy["formats"].items():
        require(0 < budget["min_words"] <= budget["max_words"], f"Invalid budget: {name}")

    examples = json.loads((ROOT / "fixtures/editorial-examples.json").read_text())
    require({x["format"] for x in examples} == set(policy["formats"]), "Missing format example")
    for example in examples:
        # Fixture check only; the production Unicode counting contract lands in #28.
        count = len(example["body"].split())
        budget = policy["formats"][example["format"]]
        require(budget["min_words"] <= count <= budget["max_words"],
                f"Example outside budget: {example['format']} ({count})")
        require(example["synthetic"] is True, "Unmarked synthetic example")
        print(f"Example {example['format']}: {count} whitespace-delimited words")

    fixture = json.loads((ROOT / "fixtures/legacy-stories.json").read_text())
    require(fixture["synthetic"] is True, "Legacy fixture must be synthetic")
    rows = fixture["stories"]
    ids = [row["id"] for row in rows]
    require(len(set(ids)) == len(ids), "Duplicate fixture row IDs")
    for row in rows:
        for source in row.get("sources_json") or []:
            host = urlparse(source["url"]).hostname or ""
            require(host.endswith(".invalid"), "Fixture contains a non-synthetic source URL")
    require(any(row.get("title_fr") is None for row in rows), "Missing null translation case")
    require(any(row.get("related_ids") for row in rows), "Missing related story case")
    require(any(row.get("source_count", 0) > row.get("origin_count", 0) for row in rows),
            "Missing syndication case")
    print(f"PASS: {len(lock['files'])} pinned GADS files; editorial settings; {len(rows)} legacy rows")


if __name__ == "__main__":
    main()
