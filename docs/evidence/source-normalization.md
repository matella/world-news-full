# Source normalization evidence (#19)

Scope: staged on #60; internal service and additive migration 8. No merge, live
migration, source subscription, article request, account creation or deployment.
No dependency changes. GADS-managed files are unchanged; project rules document
identity/provenance restrictions for both Codex and Claude Code.

Checks (final outcomes recorded before PR opening):

- `cargo test -p world-news --locked`: 30 unit/in-process tests passed; eight opt-in
  DB suites are deliberately ignored here and run by the disposable wrapper.
- `cargo fmt --all -- --check`,
  `cargo clippy -p world-news --locked --all-targets -- -D warnings`,
  `cargo build -p world-news --locked`: passed.
- `python3 scripts/test_database.py`: eight fresh database suites passed, followed
  by real migration/import/auth CLI, separated HTTP credentials, outage/readiness
  and graceful shutdown checks. All data/accounts are synthetic and task-local.
- `python3 scripts/test_rust_runtime.py`,
  `python3 scripts/check_foundation.py`, `python3 scripts/check_secrets.py`,
  `git diff --check`: passed. Secret guard is a narrow tracked-file pattern check.

Normalization assertions: concurrent tracking variants yield one article; retries
return the same receipt; changed metadata preserves first title; same headline at
other URLs and syndicated outlet URLs stay separate; parser ID changes and the
same URL in a second feed do not create duplicates. Invalid language becomes und.
Missing/unsafe/ambiguous links and oversize titles quarantine. Legacy exact URLs
recognize existing identities; multiple matches/different sources quarantine.
Disabled source/feed holds without receipt; changed source version quarantines.
Empty feeds produce a receipt. Hash and context mismatches reject without consuming
input. Injected observation and receipt errors leave no partial new articles.
Source article body/fetched_url remain NULL, attribution is captured, and no story
or article-origin row is invented. Ledgers cannot be rewritten/deleted; reader
cannot inspect them or invoke lock functions; restricted worker cannot update
articles/config, access identities, create schema objects or write stories.

Limits: metadata changes do not inspect body content; this does not certify
syndication, original reporting, real source rights or redirect equivalence. No
retroactive reconciliation of unknown historical tracking links. Archive import
requires its write freeze. Initial normalization is serial across batches, with
3-second lock and 10-second per-statement budgets; production throughput remains
#45. Jobs/recovery/operator review and complete editorial UI remain #20–#29.
No production role/grant or migration timing test. Full-workspace checks are CI;
local checks target world-news under the documented disk-limited build profile.
Live Claude Code instruction enforcement remains unverified as documented in #3.

Rollback: stop the worker and use a reviewed matching-schema forward fix or verified
restore (#47). Never drop retained observations as an automatic down migration.
