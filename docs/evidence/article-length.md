# Article-length evidence (#28)

Built on merged #75. Count-only rules, configurable language/format ranges,
immutable assessment/override revisions, active-policy publication checks, private
counter/form, public estimates and migration 17's allowlisted projection are complete.
No provider, source fetch, publishing endpoint or live schema/grant/writer action.

Passed locally:

- `cargo test -p world-news --locked --lib`: 51 unit/in-process tests.
- `cargo fmt --all -- --check`, strict all-target Clippy and locked Rust build.
- Focused new length DB integration and final complete
  `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`:
  all 19 isolated DB suites, final CLI/HTTP/auth/readiness checks and five Chromium
  workflows passed. The private mobile screenshot was visually inspected.
- Runtime smoke, foundation/GADS drift, deployment-policy check and its four
  tests, narrow tracked-file secret-pattern guard and diff whitespace check.

This small VFS Docker workspace exhausted temporary disk during fixture creation/
compilation before DB assertions could execute. Only task-owned regenerable caches
and test executables were cleaned. No host prune, existing service/data operation,
test skip or weakened assertion was used. Local dev/test profiles used optimization
level 1, 16 codegen units, stripped symbols, no debug info/incremental output and two
build jobs. These capacity settings are outside the repository. The optional fixture
cleanup removes a completed suite's own executable; CI keeps its ordinary defaults.
Disposable tmpfs does not certify disk persistence or replace the separate drill.

Unit cases cover exact default boundaries, format/language resolution, Unicode NFC
and residual/isolated combining marks, French apostrophes, modifier apostrophes,
hyphen variants, numeric separators, punctuation/emoji, authored headings/section
children, excluded metadata, configurable reading-time rounding and zero-rate errors.
Policy file tests verify size/encoding validation and path-redacted failures.

The new integration suite verifies 79-word incomplete drafts, required public
reasons, CSRF/Origin, oversized input and stale versions, approval invalidation,
unchanged prose and exact citation snapshots, old publication retention, changed
policy rejection/reassessment, unsupported-language honesty, zero-word rejection
despite an override, injected head-update rollback and live editor revocation.
Existing publishing fixtures explicitly declare their intentionally short synthetic
content; ordinary intake remains incomplete, with no fabricated exception reason.

The browser submits a reason by keyboard through the restricted editor role,
checks body preservation, count/range/warnings and mobile layout, and verifies that
anonymous readers get estimates rather than operator controls. Unsupported language
pages omit a numeric estimate; punctuation-only drafts remain 404. Private original
evidence is not rendered. All browser requests remain loopback.

Self-review checked server-computed counts/fingerprints, current configured policy,
exact revision/session/role boundaries, immutable provenance/citations, field privacy
and compatible rollback writers. It is not independent review or a quality/truth
certificate. Complete editing/publishing web acceptance remains #29, edition budgets
#30, richer reader design #34–#36 and release/recovery evidence #47–#49. Old writer
images that bypass the gates are not an acceptable rollback after gated publication.
