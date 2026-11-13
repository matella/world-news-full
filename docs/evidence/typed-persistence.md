# Typed persistence verification

Scope: #13, stacked on schema PR #53. R2: typed persistence and additive source
identity protection. No HTTP mutation/authentication or live database change.

Passed locally:

- `cargo test -p world-news --locked`: 12 unit/in-process tests passed; DB suite
  intentionally ignored by this command and executed explicitly below.
- `scripts/test_database.py` under the local Cargo environment: opt-in integration
  suite passed against disposable Postgres 18 Alpine, followed by real CLI/readiness.
- Exercised v1 -> v2 -> v3 upgrade, including source records created before v3;
  later identity edits failed while old migration checksums remained unchanged.
- Typed story/source/snapshot reads, archive keyset pagination, draft exclusion,
  withdrawn snapshot entries and reader-role denial passed.
- Duplicate draft conflict, injected mid-transaction failure with no surviving
  partial story, malformed stored blocks and credential-bearing stored URL rejection
  passed. Closed-pool unit test distinguishes unavailable DB from an empty archive.
- Strict affected-crate Clippy, formatting, foundation/secret/diff checks passed.

During a rerun the disposable database failed during bootstrap as local caches
filled the 8 GB executor. The wrapper now captures bounded stdout/stderr logs on
early exit/timeout. Removing only stale task-generated Rust artifacts and rebuilding
the affected crate restored sufficient space; the same suite then passed. Remote
CI retains full-workspace validation on a larger runner.

The repository returns errors without driver causes or payloads; public reads use
restricted views and bound parameters. This is persistence, not a replacement for
the future authorised editorial service. No word-budget enforcement, writer UI,
claim verification or production-role provisioning is claimed. See persistence-api
documentation for limits and source-identity tradeoffs.

Rollback app code separately from schema. Migration 3 is additive and retains old
data; no automatic down/drop operation exists. Exact-version readiness requires the
matching binary/migration set. Main remains unchanged and no deployment occurred.
