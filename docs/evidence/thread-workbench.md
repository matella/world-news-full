# Private thread workbench evidence (#29 service slice)

Passed locally on 2026-11-14:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all 19 disposable database suites and six existing Chromium regressions passed.
- `cargo fmt --all -- --check`, `cargo clippy -p world-news --all-targets --locked -- -D warnings` and `cargo build -p world-news --locked`.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`, `python3 scripts/check_deployment_policy.py`, `python3 scripts/check_secrets.py` and `git diff --check`.

Real restricted-role thread integration now verifies private card pagination,
retained story metadata choices, actor/session boundaries and inverse eligibility
before/after changes. It assigns an explicitly unknown instant, observes SQL NULL,
undoes and redoes without fabricating a date or changing bookkeeping added_at,
records a known nanosecond input at database microsecond precision, then corrects
it to explicitly unknown. Restored semantic membership still has a higher version;
an old action remains ineligible rather than overwriting later work. Private-only
subjects remain publicly unresolved, and reader pools/revoked sessions cannot read
workbench data. The fixture explicitly grants private current-revision metadata
reads; no live role is changed.

No new curation HTTP endpoint or browser completion is claimed. Existing browser
journeys are regressions only; full thread forms and their browser acceptance remain
#29. All known Rust assign callers were updated to the optional-instant API. Stored
nullable fields/snapshots already support it, so no schema change or data rewrite is
needed. The source-registry ADR reference was corrected to ADR 0005.

Self-review covered explicit missing metadata, bounded selections, private/public
count separation, authoritative database snapshots, optimistic preview semantics,
null-preserving UTC/microsecond precision and final undo fencing. No assertion was
skipped or weakened. This is not independent review or full publishing acceptance.
No dependency, provider, source HTTP, production or article-publication operation.
Rollback retains unknown instants and all graph/audit history; old readers already
support these nullable values and must never replace them with invented dates.

Local profile/cache/fixture settings follow earlier editor evidence; no host Docker
prune occurred. Unchanged frontend checks are not run locally in this small workspace;
GitHub workspace/frontend checks must pass before merge.
