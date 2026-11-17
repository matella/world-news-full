# Private edition draft service evidence (#30 persistence slice)

Focused local verification passed on 2026-11-17:

- Strict all-target Clippy and `cargo test -p world-news --locked --test editions --no-run`.
- `cargo test -p world-news --locked --test editions -- --ignored --test-threads=1` against a uniquely named disposable loopback PostgreSQL database: passed.

The new restricted-writer integration verifies migration 18 to 19 with existing
published membership and reader grants, preserved zero/unknown historical fences,
no fabricated import actions, real editorial authentication, private draft identity
uniqueness, explicit order/non-first lead, atomic concurrent-save rejection,
immutable private history/cursors, private/withdrawn/stale article rejection,
explicit fence refresh, preserved immutable archive membership and no public
edition from a draft save. An injected audit failure after membership replacement
rolls back entries, version and journal together. Unknown historical fences and
configured timezone drift remain visible rather than silently repaired.

The draft fixture writer cannot update edition publication columns, and anonymous
readers cannot query draft memberships or private actions. A synthetic oversized
stored body is rejected by the 8 MiB aggregate preview fence before decoding, with
previous membership/version retained. Revoked live identities cannot read/create
through an old authenticated session. Owner-only synthetic publication rows isolate
selection testing; they do not claim to reproduce article review or create a live
publication. The parent article workflow has its separate complete journey.

Full regression and final-check outcomes are recorded below before PR publication.
No test is disabled or weakened. The new opt-in suite is wired into the existing
full runner; default Cargo tests still deliberately skip disposable-DB suites.

Self-review covers identity/session/edition/story lock ordering, exact publication
pointers/versions, complete rollback, before/after actor history, immutable archives,
unknown import metadata, DB versus host observation clocks and pre-decode bounds.
This is self-review, not independent review. There is no HTTP publishing endpoint,
provider/network call, live migration, grants/accounts, deployment or writer switch.

Migration 19 adds only private version/fence/history data and preserves reader view
shapes/grants. Existing rows get no invented actors or preparation fences. Exact
schema readiness means rollback needs a schema-19-compatible binary/forward fix or
separately verified restore; do not drop history or reset published snapshots.
Protected edition browsing/selection UI and transactional edition publication remain
#30/#31 gates. This service never automatically publishes a private article.

## Full regression

`WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`
passed all 21 disposable PostgreSQL suites and eight Chromium journeys on this
source, including the complete parent author/review/publish/correct/withdraw flow.
No assertions were skipped or weakened. Foundation, full GADS audit (35 passed,
zero failures/warnings), deployment-policy, secret-pattern and diff guards passed.
Final fmt/Clippy/unit/build/process checks follow before PR publication. Unchanged
frontend and persistence checks must pass in GitHub before merge.

Self-review after that broad run found a date boundary mismatch: NaiveDate can
represent years outside the public route's canonical 0001–9999 range. Draft creation
now rejects those years; focused integration asserts years 0 and 10000 are rejected.
The final focused test and Rust checks below follow this guard; the full regression
above preceded this final narrow validation change. No imported dates are rewritten.

Final focused disposable `editions` integration passed after the canonical-date
guard, followed by `cargo fmt --all -- --check`,
`cargo test -p world-news --locked --lib` (61 passed),
`cargo clippy -p world-news --all-targets --locked -- -D warnings`,
`cargo build -p world-news --locked` and `python3 scripts/test_rust_runtime.py`.
All passed. Staged foundation/deployment-policy/secret/diff guards passed. GitHub
workspace/database/frontend/persistence checks remain required before merge.

Further premerge review found that imported publication-event created_at is the
import bookkeeping clock, not original publication. The query now excludes imported
events from age calculation. A new integration case runs the real legacy importer,
selects its actual revision/version and requires unknown publication time, without
stale/future factual freshness claims. This follow-up is verified before updating
the open PR head; the earlier counts above precede this final semantic correction.

The import-time correction passed the focused restricted-writer integration, then
the entire 21-suite PostgreSQL/eight-Chromium runner, then final fmt, 61 unit tests,
strict all-target Clippy, build and process-runtime checks. All passed on the
corrected source before the PR branch update. The real import fixture retains
unknown publication age instead of using the import clock.
