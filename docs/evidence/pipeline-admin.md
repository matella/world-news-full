# Private pipeline administration evidence (#22)

Based on #66; staged only. Migration 13 adds query indexes; no dependency, live
role/account/grant, source subscription, deployment, merge, Jarvis action or paid
provider call is introduced. Existing service and public-reader boundaries remain.

Recorded checks:

- `cargo test -p world-news --locked`: 36 unit/in-process tests passed; opt-in
  disposable database suites are separate from this count.
- `cargo fmt --all -- --check`, `cargo clippy -p world-news --locked --all-targets
  -- -D warnings`, `cargo build -p world-news --locked`: passed.
- `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all fourteen
  isolated DB suites passed, including the preceding thirteen; final schema/import/
  account CLI, HTTP/outage/readiness and both real Chromium workflows passed.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. Secret guard is a
  narrow tracked-file pattern check, not a full security/history audit.

The new isolated admin suite verifies anonymous/editor/publisher boundaries, private
page reads with restricted writer/public roles, source markup escaping, actual stale
feed warnings, failed normalization -> audited replay -> available source article,
stale replay conflicts and CSRF/origin/duplicate-token denials without side effects.
Manual source links/notes/original drafts persist atomically, remain private and
link back to the source document. Stale note forms conflict; unknown rights cannot
grant extraction. An eligible publisher can grant/cancel without creating a source
HTTP attempt. Source policy removal requires a concrete first-step confirmation,
rejects missing consent/stale versions, removes retained text and preserves removal
history. Snapshot views withhold removed text. Keyset/filter/empty/missing/outage
states, private CSS/CSP, role changes, session revocation and public-role private
read denial are checked. No publication event is created by the workflow.

The Chromium fixture uses only synthetic local accounts/URLs. It signs in, creates
a source and disabled feed, verifies the feed URL stays read-only, registers a manual source link and original notes, creates an original
private draft and verifies its public URL is 404. It checks mobile table overflow,
real browser form/Origin behavior, no external requests and no credentials/draft
title in logs. Screenshots of the draft and overview were visually inspected at
1280px. Explicit control labels were added after browser testing exposed ambiguous
select labels; fixture escape expectations use Askama's numeric HTML entities.

The wrapper drops completed isolated test databases after their checks when no
later process/browser fixture needs them. It retains the base/import/auth/admin
fixtures until their final checks and removes its unique container/volumes in
finally. This avoids archive-sized accumulation on constrained test workspaces;
no application data cleanup behavior is changed. Initial runs hit local linker/
disk exhaustion before all checks; they are not counted as successful runs.

Self-review covers live reauthorization, fixed action paths, strict forms, SQL
bindings, source URL validation, escaping, service reuse and text-removal consent.
This is not an independent human review, production grant audit, full accessibility
certification, archive-scale performance test or live operational acceptance.

Limits: detailed history shows the latest twenty records; full editorial editing,
review/publication, citations/word budgets and edition composition remain #26–#31.
The worker stays explicit; visiting an admin page does not fetch, schedule or
publish. Counts/freshness describe observations, not a whole-pipeline health claim.
Claude Code runtime enforcement and live cutover evidence remain unverified.
Rollback removes the staged UI from service while preserving jobs, drafts and audit
history; do not reverse migrations by deleting data or widening reader privileges.
