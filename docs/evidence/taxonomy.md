# Editorial taxonomy evidence (#24)

Based on #68; staged only. No schema migration, dependency, live backfill/grant,
source fetch, publication, deployment or writer action is introduced. Historical
revision/payload/ledger/snapshot data remain immutable.

Final checks:

- `cargo test -p world-news --locked`: 40 unit/in-process tests passed.
- Strict `cargo fmt --all -- --check`, `cargo clippy -p world-news --locked
  --all-targets -- -D warnings` and `cargo build -p world-news --locked`: passed.
- `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all fourteen
  disposable DB suites passed, followed by migration/import/auth/process/HTTP and
  both Chromium workflows, including original draft classification.
- Frontend `npm run typecheck`, `npm run lint`, `npm test` (five tests), `npm run
  build`, and `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_frontend.py`: passed.
  The browser verifies French science appears in the science section rather than
  being grouped by language. No dependency/lockfile changed.
- Runtime smoke, foundation/GADS drift, tracked-file secret guard and diff checks:
  passed. These guards are not a complete security/history audit.

The generated-copy drift guard preserves the frontend-only Docker context after
an initial cross-context JSON import failed to build. The final build uses the
checked generated snapshot and the unchanged existing deployment contexts.

The taxonomy tests cover the five requested editorial fixtures, unknowns/aliases,
duplicate/invalid keys, unknown schema fields/references, policy size, deterministic
rule precedence and changed config fingerprints. The file test checks a real
bounded override and redacted missing-file/oversize errors.

The protected admin suite constructs a synthetic historically published legacy
revision, performs an editor classification override and checks language/kind/
content/provenance/citations are preserved, public metadata still uses the old
revision, review resets and classification provenance records the policy hash.
Stale/duplicate/unknown fields are rejected. An injected citation-copy failure rolls
back the new revision/head. The real Chromium source/link/original-draft workflow
now also submits classification checkboxes and observes the proposed Belgian section
while the draft remains publicly unavailable. The classification screen screenshot was also visually inspected; focused and full
SQL/Chromium checks passed.

The import integration verifies policy fingerprints and unknown geography in the
proposal. Dry-run/apply/already-imported mappings remain proposals; no silent
backfill or invented certainty is claimed. Existing archive hashes/data/provenance
are preserved, and prior importer rollback/idempotence checks remain applicable.

Self-review is not independent review, full accessibility certification, archive-
scale performance evidence or actual live archive reconciliation. Configurable
section rules are the shared API for future edition/search work (#30/#38); those
consumers are not claimed complete. Live bulk backfill and operational cutover stay
under #47–#49, and Claude Code runtime enforcement remains unverified.
