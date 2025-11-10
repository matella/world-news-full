# Deploy to k3s — Extended README

This extended README includes the self-hosted runner option and references to the added k8s manifest templates.

What I added

- `k8s/frontend-deployment.yaml` and `k8s/ai-service-deployment.yaml` — basic Kubernetes Deployment templates. Edit the `image` fields to match your registry/tag or keep as-is if you will load images locally into k3s.
- `.github/workflows/deploy-selfhosted.yml` — an alternative workflow that runs on a self-hosted runner. When a runner is registered on your server this workflow will build and deploy locally (no SSH transfer).
- `scripts/install-runner.sh` — a helper showing how to download/extract the GitHub Actions runner; you still need to register it with a token from GitHub.

Self-hosted runner quick setup (high level)

1. Create a runner in your repository: GitHub → Settings → Actions → Runners → Add runner. Copy the registration token.
2. On your server, run the commands (example for Linux x86_64):

```bash
mkdir -p ~/actions-runner && cd ~/actions-runner
curl -O -L https://github.com/actions/runner/releases/download/v2.310.0/actions-runner-linux-x64-2.310.0.tar.gz
tar xzf ./actions-runner-linux-x64-2.310.0.tar.gz
./config.sh --url https://github.com/<OWNER>/<REPO> --token <REGISTRATION_TOKEN>
# start the runner in background or install as a service
./run.sh &
```

3. Ensure the runner user can run `docker build`, `kubectl`, and access k3s (or run the runner as a user with those permissions). Optionally install the runner as a systemd service (the GitHub docs show how).

How the self-hosted workflow works

- The workflow uses `dorny/paths-filter` to detect which project changed and only runs the corresponding job(s).
- Jobs run on a self-hosted runner (so they execute on your server). The job simply checks out the repo and runs `./scripts/deploy.sh <project> <sha>`.
- Because the build runs locally on your k3s host, there's no need to push images to a registry or transfer them over SSH; the script builds the image and loads it into k3s containerd.

Notes and recommendations

- If you plan to use the GitHub-hosted workflow (SSH-based), keep the existing `.github/workflows/deploy.yml` and use the SSH secrets approach.
- If you plan to use a self-hosted runner, consider running the runner under a dedicated user (e.g. `deploy`), add that user to the `docker` group, and allow it to use `kubectl`/`k3s` as needed.
- For production, prefer pushing images to a private registry and configure k3s to pull from it. Local image import is convenient for development but less robust for multi-node clusters.

Next possible improvements (I can add any of these):

- Add resource requests/limits and an explicit image tag placeholder in the k8s manifests.
- Add a small script to push images to a local registry and update manifests to reference the registry.
- Add a systemd unit file example to run the actions runner as a service.

Tell me which of the above you'd like next and I'll implement it.
