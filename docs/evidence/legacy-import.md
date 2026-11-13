# Legacy import verification

Scope: #12, stacked on typed-persistence PR #54. Offline planning and an explicit
write-capable historical import; no live database, Jarvis or deployment action.

Passed locally:

- `cargo fmt --all -- --check` and affected-crate strict Clippy.
- `cargo test -p world-news --locked`: 16 unit/in-process tests passed. The two
  intentionally ignored DB suites were separately executed by the wrapper below.
- `cargo build -p world-news --locked` followed by `python3 scripts/test_database.py`:
  schema suite and fresh-DB legacy import suite passed on disposable Postgres 18.
- Injected failure after some batch writes left zero stories, sources, source
  articles, ledger rows and editions. Removing the synthetic trigger allowed the
  same plan to commit: three valid stories/three editions, one quarantined fixture.
- Exact reapply inserted nothing, retained IDs/mappings, and created no editions.
  Changed records and new rows for an occupied immutable edition were quarantined.
- Preserved title/language/French variant, correction metadata, reported counts,
  source URLs and related hints. No reporting origins, threads or fetched outlet
  bodies were fabricated. Reader-role apply was denied without surviving writes.
- Real CLI default dry run used an empty environment (no DATABASE_URL), emitted
  metadata JSON without fixture body/title, and exited 2 for quarantine. Explicit
  apply rerun reported three already imported records and exited 2 as documented.
- Unit coverage includes credential-bearing URLs, conflicting duplicates, stable
  key-order hashing, exact large JSON integers, unsupported NUL/numeric ranges,
  and whole-edition overflow quarantine rather than silent truncation.

SHA-256 reuses the existing locked sha2 version. serde_json arbitrary_precision
retains legacy numeric metadata rather than rounding it during fingerprint/storage.

Not run: actual production export/schema inspection, full archive reconciliation,
live freeze/import, backup/restore or reader cutover. Input limits, unverified
legacy provenance and the fresh-target final-import strategy are documented in
`docs/legacy-import.md`. Reports and original files require private retention.

Rollback before deployment is reverting this application change. No schema is
added here; applied historical data has no automatic destructive undo command.
Retain the original legacy DB and use the explicit backup/cutover plan, never
delete curated data to make a rerun succeed.
