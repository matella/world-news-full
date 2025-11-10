# World News — Local Skeleton (Anonymous Mode)

This repository contains a local development skeleton for the World News project.
It runs entirely locally and anonymously (no authentication) and is intended for
development and environment verification.

## Structure

- `backend/` — Rust workspace with three services: `api-gateway`, `scraper`, `ai-service`
- `frontend/` — Next.js + TypeScript + Tailwind minimal app that queries the GraphQL API
- `docker/` — Docker Compose and Dockerfiles for local runs

## Quick start (recommended)

Requirements: Docker & Docker Compose (v2), Node.js and Rust toolchain for local runs.

### Using Docker Compose (builds dev containers)

```bash
cd docker
docker compose up --build
```

This will start:

- Postgres (for later use)
- API Gateway on port 8000
- AI service on port 9000
- Scraper (runs mock loop)
- Frontend dev server on port 3000

### Running frontend locally (recommended for fast iteration)

```bash
cd frontend
npm install
npm run dev
```

### Running Rust services locally

Each backend service is a standalone Cargo project:

```bash
# api-gateway
cd backend/api-gateway
cargo run

# scraper
cd backend/scraper
cargo run

# ai-service
cd backend/ai-service
cargo run
```

## Notes

- All services are minimal skeletons with mock behavior suitable to validate your environment.
- The frontend includes a small test page that calls `POST http://localhost:8000/graphql` with `{ hello }` query.
- Read individual READMEs in each service for more details and developer prompts.
