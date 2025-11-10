# AI Service (backend/ai-service)

Purpose: Provide a local REST endpoint to process raw article text and return structured outputs (title, summary, body).
Currently implemented as a mock. Future work: integrate Ollama or llama.cpp and implement RAG workflow.

How to run:
```bash
cd backend/ai-service
cargo run
```

User stories covered:
- Process new raw articles into summarized processed_articles
- Store AI metadata and provenance
- Provide reprocess endpoint for manual/admin re-runs

Prompting template (for future changes):
```
You are an expert in Rust and service design. Implement an endpoint POST /process that
accepts JSON { text: string } and returns { title, summary, body }. Use streaming where possible
and include detailed structured logs.
```
