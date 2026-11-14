#!/usr/bin/env python3
"""Narrow source guard: code integration must not invoke legacy host deployments."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_MANUAL = 'name: Release held during Rust migration\n\non: workflow_dispatch\n\npermissions:\n  contents: read\n\njobs:\n  release-not-configured:\n    runs-on: ubuntu-latest\n    timeout-minutes: 1\n    steps:\n      - name: Explain the release gate\n        run: |\n          echo "Legacy deployment automation is retired. No remote action was performed."\n          echo "Use docs/operations/code-integration.md and the reviewed release plan."\n          exit 1\n'


def check(root):
    workflows = root / ".github/workflows"
    for retired in ("deploy.yml", "deploy-selfhosted.yml"):
        if (workflows / retired).exists():
            raise ValueError("Retired automatic deployment workflow was reintroduced: " + retired)
    if (workflows / "release-guard.yml").read_text() != EXPECTED_MANUAL:
        raise ValueError("The manual release hold changed; review the release policy explicitly")
    forbidden = ("self-hosted", "scripts/deploy.sh", "docker system prune", "docker compose", "docker-compose", "kubectl")
    for path in workflows.glob("*.y*ml"):
        text = path.read_text().lower()
        if any(token in text for token in forbidden):
            raise ValueError("Workflow contains a legacy host/deployment path: " + path.name)


if __name__ == "__main__":
    check(ROOT)
    print("PASS: legacy deployment triggers retired; exact manual hold; no known host/deploy commands in workflows")
