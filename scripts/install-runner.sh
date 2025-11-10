#!/usr/bin/env bash
set -euo pipefail

# Helper script to download and extract a GitHub Actions self-hosted runner.
# You still must register the runner with a registration token from GitHub.
# See the README-deploy.md for full instructions.

ARCH="$(uname -m)"
OS="$(uname | tr '[:upper:]' '[:lower:]')"

echo "Detected OS=$OS ARCH=$ARCH"
echo "This script downloads the runner archive for the current platform and extracts it into ./actions-runner"

mkdir -p actions-runner
cd actions-runner

# Determine latest runner (simple approach: use a stable tarball name; user may want to override)
RUNNER_VERSION="latest"
echo "Please download the latest runner from https://github.com/actions/runner/releases and follow GitHub's registration flow."
echo "Example commands (replace <OWNER> and <REPO> and get a registration token from GitHub):"
cat <<'EOF'
# download and extract (example for x64 Linux):
mkdir -p actions-runner && cd actions-runner
curl -O -L https://github.com/actions/runner/releases/download/v2.310.0/actions-runner-linux-x64-2.310.0.tar.gz
tar xzf ./actions-runner-linux-x64-2.310.0.tar.gz

# register the runner (you need a registration token from GitHub)
# ./config.sh --url https://github.com/<OWNER>/<REPO> --token <REGISTRATION_TOKEN>

# start the runner (or install as a service)
# ./run.sh
EOF

echo "After you register the runner, ensure the runner has permissions to run Docker, kubectl, and k3s (if needed)."
