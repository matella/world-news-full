#!/usr/bin/env python3
"""Exercise the source guard in disposable directories; no Docker or remote host."""
import tempfile
import unittest
from pathlib import Path
from check_deployment_policy import EXPECTED_MANUAL, check


class DeploymentPolicyTests(unittest.TestCase):
    def fixture(self, root):
        workflows = root / ".github/workflows"
        workflows.mkdir(parents=True)
        (workflows / "release-guard.yml").write_text(EXPECTED_MANUAL)
        return workflows

    def test_manual_hold_and_ordinary_ci_are_allowed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflows = self.fixture(root)
            (workflows / "checks.yml").write_text("on: pull_request\njobs:\n  check:\n    runs-on: ubuntu-latest\n")
            check(root)

    def test_retired_workflow_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflows = self.fixture(root)
            (workflows / "deploy.yml").write_text("on: push\n")
            with self.assertRaises(ValueError):
                check(root)

    def test_changed_manual_hold_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflows = self.fixture(root)
            (workflows / "release-guard.yml").write_text(EXPECTED_MANUAL.replace("workflow_dispatch", "push"))
            with self.assertRaises(ValueError):
                check(root)

    def test_known_host_or_deploy_commands_are_rejected(self):
        for text in ("runs-on: self-hosted", "run: ./scripts/deploy.sh frontend", "run: docker system prune -f", "run: kubectl apply -f k8s/"):
            with self.subTest(text=text), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                workflows = self.fixture(root)
                (workflows / "unexpected.yml").write_text(text)
                with self.assertRaises(ValueError):
                    check(root)


if __name__ == "__main__":
    unittest.main()
