# Stable subject thread evidence (#25 core)

Based on #69; staged only. Migration 14 extends the owned model, with no dependency,
source fetch/publication, live reconciliation/grant, reader switch or deployment.
No legacy similarity/related-ID graph is automatically promoted to an identity.

Recorded checks:

- Strict fmt/Clippy/build and 40 existing Rust unit/in-process tests passed.
- WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py: all fifteen isolated
  DB suites passed, including the new thread suite and previous fourteen, followed
  by CLI/HTTP/auth/readiness/shutdown and both existing Chromium workflows.
- The final suite includes a forced resolve/read merge race: a fixture gate pauses
  alias resolution, another transaction merges the subject, and the reader retains
  a consistent old timeline rather than a false empty page. A fresh read sees the
  new canonical timeline. The gate/function definition are restored in the fixture.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. Secret checking is
  a narrow tracked-file pattern guard, not a complete security/history audit.

The new suite first upgrades from schema 13. Ambiguous old primary assignments
cause migration rejection with both assignments retained. Only explicit fixture
reconciliation permits retry, and an old member's update instant stays unknown.

Restricted-role service checks cover one deterministic head per subject, same-time
ID ties, cross-day memberships, distinct edition/date counts (including one story
selected in two editions), NULL-time keyset pages, legacy taxonomy aliases and
combined same-update language/geography filters. Private/withdrawn future members
cannot become a head or inflate public counts. Another primary assignment conflicts.

Merge aliases, split reversal, undo/redo, retained empty closed identities and stale
inverse conflicts are exercised. Concurrent expected-version edits permit exactly
one winner. Injected action-insertion failure rolls back memberships/title/version
and audit together. Capacity beyond 200 is rejected. Twenty-edge alias resolution
works, deeper merges and cycles through merged aliases are rejected. The reader
cannot inspect/write private tables; the writer has no default resolver execution.
Private history is bounded and action-row updates are rejected.

Cyclic/overlapping/duplicate/missing legacy hints are imported as deduplicated inert
links without creating threads. An initial fixture lacked required edition dates;
it was corrected before the focused pass. Live publisher-role and session revocation deny
further mutation. Batched membership writes preserve exact before/after instants
and atomic rollback, without one insert round trip per member. Full session/CSRF boundaries remain in the preceding auth/UI tests;
no new HTTP mutation endpoint is exposed by this core API slice.

Self-review is not independent human review, production grant review, archive-scale
performance evidence or reader/editor UI acceptance. Editor controls #29, search
#38 and stable timelines/topics #39 remain explicit integration gates. The old
Next related-ID presentation is not claimed migrated by a Rust service alone.
Rollback retains stable IDs/history and requires the later release/restore gates.
