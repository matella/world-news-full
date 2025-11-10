# API Gateway (backend/api-gateway)

Purpose: Expose a GraphQL API for articles, regions and sources. This skeleton exposes a `hello` query
and a simple GraphiQL UI at `http://localhost:8000/graphiql`.

How to run:
```bash
cd backend/api-gateway
cargo run
```

This will start the service on `http://127.0.0.1:8000`.

User stories covered by this service (Anonymous phase):
- Provide article feed endpoints (articles query)
- Serve single article details (articleById)
- Support cursor-based pagination for feeds (articles connection)
- Expose sources and regions for frontend filtering

Prompting template (when asking for feature additions):
```
You are an expert in Rust backend development using Axum and async-graphql.
Add [feature X] following non-blocking async patterns, structured logging (tracing),
and DataLoader-style batching to avoid N+1 database issues.
```
