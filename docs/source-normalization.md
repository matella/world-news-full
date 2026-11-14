# Source metadata normalization

`Normalizer::new(worker_pool).apply(attempt_id)` processes a durable modified-feed
batch created by `FeedPoller`. It is an internal service, not an anonymous HTTP
route or startup loop. Jobs/recovery (#21), operator review (#22), extraction/manual
intake (#20) and claim/origin review (#27) remain separate.

The service verifies schema-1 metadata hashes, attempt/source/feed/version context,
terminal fetch status and entry counts. It locks source then feed configuration and
holds disabled input; version changes quarantine entries. Application errors roll
back the entire batch. Receipts make retries deterministic, including counters and
article IDs in entry order (IDs can repeat when entries identify the same article).
Empty batches receive receipts; a batch reason also records configuration changes.

See ADR 0007 for exact URL matching. A hash index supports legacy exact-URL lookup
without a B-tree text size limit. Unsafe/missing/ambiguous links, missing/empty or
more than 500-character titles and identity conflicts quarantine rather than invent
content or truncate. A title is trimmed; the original is preserved in the ledger.
Invalid/missing language becomes `und`; valid tags indicate parser input, not
verified language or geography. The normalizer does no DNS resolution: URL syntax
validation is not fetch-time SSRF protection. Extraction must use SafeHttp.

Provenance is explicit:

- `source_articles.original_url` is the first eligible input link, tracking intact;
  all links, parser IDs, dates and author names remain in private observations.
- `canonical_url`, key and normalizer version document the matching decision.
- `published_at` is parser-supplied, not independently verified. Feed batch
  `final_url` is the URL actually fetched for the feed, not the article.
- Article `fetched_url` and body remain NULL. Existing `fetched_at` records metadata
  ingestion here, with that semantic recorded in metadata. Observation `observed_at`
  is feed completion time and `ingested_at` is normalization time.
- Captured outlet labels mean configured source attribution, not original publisher
  verification. Neither syndication nor independent reporting origins are inferred.

Changed metadata is retained as `metadata_changed`; first-seen article fields stay
unchanged, even after extraction. An exact legacy match without a normalization
fingerprint is also a review observation, never an automatic rewrite. Observations
record disposition/reason, input entry, semantic hash and optional matched ID.
They contain publisher metadata and belong to the private worker/operator boundary.
Do not log raw links, metadata or article text.

An isolated worker needs USAGE on `news`, SELECT on `schema_version`, `feed_batches`,
`feed_fetch_attempts`, `source_articles`, `normalized_batches`, `source_observations`,
SELECT(id,name) on `sources` for the attribution trigger, INSERT on the three article/
observation/receipt tables, and explicit EXECUTE on `lock_source_for_fetch(text)`
and `lock_feed_for_normalization(text)`. These fixed-search-path owner functions
permit short share locks without granting UPDATE on source/feed configuration.
Neither function is executable by PUBLIC. No account, story, config UPDATE or DDL
permission is required. Tests use synthetic credentials only; no live grants applied.

Migration 8 is additive and immutable ledgers remain for audit. Stop the new worker
and run the previous application against its matching schema as part of a reviewed
release rollback; do not drop evidence or silently bypass exact schema readiness.
Restore from verified backup for a full data rollback (#47). Nothing here authorizes
migrating a live database or cutting off a live Jarvis writer.
