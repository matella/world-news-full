# Feed recovery evidence (#21, slice 2)

Staged on #63; no merge/live migration/grants/deployment, real source subscription,
traffic switch, Jarvis stop or provider call. Migration 11 uses app-owned tables.
No new dependencies or external runtime services. GADS-managed files are unchanged;
project-owned instructions document worker ownership, hints and machine provenance.

Final checks:

- `cargo test -p world-news --locked`: 34 unit/in-process tests passed; eleven opt-in
  database suites are ignored here and run separately in isolated local databases.
- `cargo clippy -p world-news --locked --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo build -p world-news --locked`: passed.
- `python3 scripts/test_database.py`: all eleven fresh database suites passed,
  including the preceding ten and the new feed-recovery suite. Final migration,
  legacy import/account CLI, separated auth/HTTP/outage/readiness and graceful
  shutdown checks passed. No production records/accounts were used.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. Secret guard is
  a narrow tracked-file pattern check, not full history/security assurance.

New feed suite assertions:

- Concurrent due work (including forced next-due advancement) sends one transport
  call while the current started lease remains unclosed. No transaction holds
  source/feed locks during transport; publisher pause during transport succeeds.
- Modified batch/cache/attempt/normalization job and exact machine parent commit
  atomically; a restricted metadata worker can process that job. 304 creates no
  extra job. No human actor/operator event is fabricated for machine enqueue.
- Expired ownership cannot advance cache. Recovering an in-flight old owner,
  completing a replacement and then releasing the old transport preserves the
  replacement's cache. Historic superseded attempts close without changing the
  current feed's control version/status. Repeated/busy-row recovery is safe.
- Injected machine-origin insertion failure rolls back new batch/job/cache and
  leaves started ownership for recovery. Crossing the deadline during a delayed
  DB completion also rolls back the whole success transaction.
- Five failures pause with an exhausted budget; publisher-reviewed resume resets
  the cycle while preserving source next-due minimum. Editor/stale control/revoked
  session requests are denied. History is bounded and private. Source changes
  cancel old completion; explicit pause invalidates its started ownership.
- u64::MAX Retry-After survives numeric comparison without timestamp overflow;
  early resume is denied. Even manually changing pause/due cannot make the worker
  bypass it. A finite long hint permits resume only after controlled time advance.
- Upgrade from schema 10 preserves unknown historic retry holds, pauses previously
  exhausted counts and makes historical started attempts eligible for recovery.
  Missing source instructions are not reconstructed or silently cleared.
- Actual child-process kill: parent starts a dedicated test binary under restricted
  worker credentials, waits for committed started reservation, kills only that
  owned child, advances its deadline deterministically and invokes the real
  --recover-feed-leases-once CLI. Recovery records lease_expired; a fresh attempt
  then succeeds. The hanging transport is trusted test injection with no network,
  not a production private-address option. The child helper is inert without its
  exact disposable-test marker and database/role guards.
- Real --poll-feed-once CLI reports a disabled configured feed with no HTTP. Reader
  cannot access attempts/audits/machine origins; worker cannot access identities,
  write stories, change sources or create DDL. Immutable origins/actions and
  finished attempts reject mutation. Zero stories are produced.

The existing storage-failure regression now explicitly expires/recovers an abandoned
attempt before retry. Resetting next due alone must no longer replace a live owner.
Initial CLI compilation needed a safe FetchError-to-CLI error mapping; final strict
checks and all DB suites use the corrected binary. Upgrade/busy-row and stale-owner
cases were added during review and the full wrapper rerun after those changes.

Limits: source HTTP behavior is tested through shared local fixtures from #18 and
trusted transport injection for these DB/process interleavings. No public-source
TLS/egress/grants test, deployed crash drill, production performance or complete
publishing acceptance is claimed. The process kill is real; lease time advance is
controlled rather than a measured 60-second outage. Old batches predating automatic
handoff can still use JobAdmin enqueue and require archive/release reconciliation;
no silent historical bulk subscription/replay is introduced. Machine extraction
authorization, scheduling/orchestration and protected admin UI retain #21/#22 gates.
Full-workspace checks are CI. Claude Code runtime enforcement remains #3.

Rollback: stop explicit invocations; use a matching-schema forward fix or verified
restore. Preserve jobs/attempts/cache/audit and conservative holds. Do not bypass exact
schema readiness, delete ownership history or discard source minimums during upgrade.
