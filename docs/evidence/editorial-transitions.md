# Editorial transition verification

Scope: #16, stacked on local-auth PR #57. Internal session-authorised services and
additive migration 5. No publishing HTTP endpoint, UI, live accounts/data or deploy.

Passed locally:

- Strict affected-crate Clippy/build and 23 unit/in-process tests.
- `python3 scripts/test_database.py`: all five opt-in fresh database suites passed,
  followed by real CLI/auth/reader checks. Resources and account/data fixtures are
  synthetic and task-owned. The new suite uses a column-granted editorial runtime
  role plus a public-view-only reader, not the DB owner for application operations.
- Real HTTP login -> live Session -> draft/save/submit/changes-request/approve/
  publish/correct/withdraw service calls. Editor denied publisher actions. No
  publication before review; missing public reason and empty review body rejected.
- Saved revision resets approval; prior revision approval cannot publish a later
  candidate. Private notes, change reasons, actors and timestamps retained; private
  review notes absent from public HTML/notices. Public reason available downstream.
- Two concurrent edits with one lock_version: exactly one success, one conflict,
  one surviving numbered revision/action. Stale expected versions never overwrite.
- Injected create failure leaves no partial story/revision/action. Injected failure
  after correction pointer/event writes rolls the entire transaction back; retry
  with the original expected version succeeds without duplicate public events.
- Public body remains the approved old revision while a correction is drafted and
  reviewed. Correction updates current reading while an old edition retains its
  selected revision. Withdrawal targets published revision even with a newer private
  draft; current/historical public body URLs disappear and edition shows a marker.
- Metadata history pages of 20 then 3 rows preserve exclusive cursor semantics.
  Revisions/actions are immutable and actor IDs come from session identity.
- Cached sessions fail after role downgrade, expiry or disabling. Identity row-lock
  concurrency test confirms a waiting action rechecks the committed role change.

The new transaction suite complements #15's CSRF/cookie/logout and Chromium checks;
it does not claim a new editor browser journey. Formatting, foundation/secret/diff
guards and real-process outage/shutdown checks passed. GitHub CI supplies full-workspace checks.

Not run/complete: actual UI, complete citations/structure/word-budget publication
policy, edition composition/publication, production grants/recovery/TLS or a real
news publication. No provider or Jarvis is used. These remain release blockers;
state-machine success is not a completed independent newspaper.

See ADR 0004/editorial-service.md for API, limits, self-review/terminal-withdrawal
choices and exact-version rollback. No destructive down migration exists.
