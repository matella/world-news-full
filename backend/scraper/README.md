# Scraper (backend/scraper)

Purpose: Periodically fetch news sources (RSS/APIs/HTML), extract canonical text and metadata,
and store them in the raw_articles table (Postgres JSONB).

How to run:
```bash
cd backend/scraper
cargo run
```

This skeleton runs in a loop and logs a message every minute. Replace the TODO with real fetch & parsing logic.

User stories covered:
- Fetch articles hourly from configured sources
- Normalize articles into raw_articles JSONB
- Compute canonical_hash for deduplication
- Provide hooks or a queue for AI processing
