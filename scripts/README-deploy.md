# Deploy to k3s — README

This file documents the simplest approach used by the included GitHub Actions workflow to build and deploy per-project changes to a k3s server.

Overview
- The workflow (`.github/workflows/deploy.yml`) runs on push to `main`.
- It uses `dorny/paths-filter` to detect which project(s) changed (frontend, backend services).
- For each changed project it SSHes to your server and runs `./scripts/deploy.sh <project> <git-sha>` inside a local clone of the repo on the server.

What you must prepare on the server
1. Install k3s (with Docker support or containerd). See https://k3s.io.
2. Install Docker (if you plan to build with Docker) and have the `docker` CLI available to the user that will run the deploy.
3. Install `kubectl` and ensure it can access the k3s cluster (kubectl context available to the deploy user).
4. Create a directory to host the repository on the server (for example `/opt/world-news`) and ensure the SSH user has read/write there.
5. Add a deploy user (or reuse an existing one) and place its SSH private key as a GitHub secret.

Required GitHub repository secrets
- `SERVER_HOST` — server IP or hostname
- `SERVER_USER` — username used for SSH
- `SERVER_SSH_KEY` — private SSH key for the user (no password) — keep as secret
- `SERVER_SSH_PORT` — SSH port (optional; set to `22` by default)
- `SERVER_REPO_DIR` — path on the server where the repo should be checked out (e.g. `/opt/world-news`)

How the workflow uses them
- The workflow will SSH to the host, `git clone` or `git fetch` into `SERVER_REPO_DIR`, then run `./scripts/deploy.sh <project> <sha>`.

Notes & alternatives
- If k3s runs containerd and cannot see Docker images, the script imports the built Docker image directly into k3s using `k3s ctr images import -` (requires `sudo`). If that does not work for your k3s setup, consider deploying to an internal container registry and configuring k3s to pull from it.
- Alternative approaches:
  - Self-hosted GitHub Actions runner on the server: workflows run directly on the server, no SSH and no image transfer required. This is often the simplest for fully-local builds.
  - Use a container registry (Docker Hub / GitHub Container Registry / private registry) and push images from Actions, then use `kubectl` to update the cluster. This avoids loading images into k3s directly but requires network/credentials and may be slower.

Next steps
1. Add the secrets to your GitHub repository (Settings → Secrets).
2. Ensure the server has `git`, `docker`, `kubectl`, and `k3s` (or the appropriate container runtime) installed and the SSH user can run the necessary commands (sudo may be required for `k3s ctr`).
3. Adjust `scripts/deploy.sh` image names, k8s deployment names, namespace, and manifest locations to match your k8s manifests.

If you'd like, I can:
- Convert this to a self-hosted-runner setup (provide runner installation commands), or
- Add `kubectl` manifest templates (k8s/...) and a small README for changing image tags in the manifests before apply.
