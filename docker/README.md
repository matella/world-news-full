# Docker Compose (docker)

This compose file is aimed at local development. It builds Rust services using the Rust official image
and runs the frontend using a Node container with a mounted volume for hot-reload.

To run:

```bash
cd docker
docker compose up --build
```

Notes:
- Building Rust images inside Docker can be slow on first run. For iterative Rust development, prefer running services with `cargo run` locally.
- Frontend uses a bind mount; running with Docker requires node/npm in the container.
