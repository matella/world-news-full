# Citation policy evidence (#27)

Built on merged #74. Migration 16 adds an explicit nested public projection and
restricted evidence-lock helper. Source packages, source-only candidate revisions,
shared private/public reports, reader footnotes and the sourcing publication gate
are implemented without a provider, source HTTP, dependency or publishing endpoint.

Passed locally:

- `cargo test -p world-news --locked`: 45 unit/in-process tests.
- `cargo fmt --all -- --check`, strict all-target Clippy and locked build.
- Focused disposable citation integration, including a deterministic blocked
  publication/origin-change race and complete source-only revision journeys.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`:
  all 18 isolated DB suites, final CLI/HTTP/auth/readiness checks and four real
  Chromium workflows. The citation screenshot was visually inspected.
- Runtime smoke, foundation/GADS drift, deployment-policy check and four policy
  tests, narrow tracked-file secret-pattern guard and diff whitespace check.

The initial broad run caught a reader-contract regression: the sources heading
had been renamed. The existing heading was restored, preserving the assertion and
source URL behaviour. The complete suite then passed. No test was skipped or its
assertion weakened. Existing publication fixtures now explicitly declare their
synthetic original methods; ordinary manual intake constructors remain incomplete
drafts. Restricted publisher fixture roles receive only the new read/helper grants.

Coverage includes two outlet records/one canonical document, syndicated documents
sharing one origin, inconsistent origin sets for one document, missing claims,
missing sources with atomic rollback, exact review invalidation, opinion/type
validation, single-source limits, genuine versus fabricated historical lineage,
prose edits dropping implicit citation authority, live editor revocation, frozen
published evidence and no private notes/actors in public projections. Nested
unexpected private fields are also excluded by the pure SQL allowlist. Human method
declarations count as methods, not fabricated external sources/origin identities.

The browser checks keyboard citation navigation, plain escaped source locations,
count definitions, public limitations, attributed disagreement positions, private
note exclusion, mobile overflow, real single-source reporting and labelled historical
metadata corrections. A fake historical draft stays 404. Requests remain loopback.
Existing private-draft tests verify missing-provenance warnings are visible.

Self-review examined role/session/version checks, immutable package/citation
boundaries, exact body anchors, bounded lists, conservative canonical identity,
fixed-search-path/revoked-PUBLIC locks and explicit public field projection. This
is not independent review, automatic truth verification or complete publishing
acceptance. Humans still identify material facts and evaluate evidence. Word/editor/
edition/reader design acceptance remains #28–#31/#34–#36/#48. No live schema/grant,
account, writer, data, deployment or traffic change occurred. Roll back with a
compatible gated writer or forward fix, never an old bypassing publishing binary.
