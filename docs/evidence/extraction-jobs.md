# Authorized extraction evidence (#21 slice 3)

Based on #64, staged only. Migration 12 adds explicit machine grants/attempts,
operator actions, shared source cooldowns and strict human-XOR-worker snapshot
origin. No dependency/service, provider, source subscription, live account/grant,
migration, deployment, merge, traffic or Jarvis action occurred.

Final local checks:

- `cargo test -p world-news --locked`: 34 unit/in-process tests passed; twelve
  opt-in database suites are ignored here and run by the disposable wrapper.
- `cargo clippy -p world-news --locked --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo build -p world-news --locked`: passed.
- `python3 scripts/test_database.py`: all twelve separate fresh DB suites passed,
  including preceding schema/import/HTTP/auth/editorial/registry/feed/normalization/
  intake/jobs/recovery coverage and the new extraction grant suite. Final CLI,
  separated auth/HTTP/outage/readiness and graceful shutdown checks passed.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. Secret guard is
  a narrow tracked-file pattern check, not a complete history/security audit.

New integration uses actual issued human sessions only in the operator/manual
pool. The machine pool cannot SELECT identities or sessions, INSERT grant/actions,
write human snapshot actor/notes, create stories, modify source config or create DDL.
A fixed-path authority function returns only the issuer's active publisher status.
A grant is an expiring audited decision, never a serialized/permanent session.

Checks: editor cannot grant; duplicate active request keeps one grant/action;
concurrent claims have one winner. Success records actor_id NULL and the exact
worker job ID; human manual notes still have the real editor and no worker ID.
Repeated completion is fenced. Three transient failures end in a failed private
snapshot/job, without an automatic original draft. Cancellation during transport,
manual edit, source version change and issuer demotion prevent stale text. A grant
that expires during a real awaited fixture transport is cancelled without intake
writes. Logout alone leaves an already-issued one-hour delegation valid; the revoked
session cannot issue another. An already expired queued grant produces zero HTTP.

Injected success-ack failure rolls back text/snapshot/intake version, leaving
started ownership for explicit expiry/recovery. A later generation completes, and
old owners cannot execute again. Rights withdrawal removes retained machine text
through the existing source trigger and preserves withheld provenance. No stories
are created. Finished attempt/grant/action context rejects mutation.

Source hint cases are reciprocal: a committed manual HTTP failure blocks a new
machine grant and links its cooldown to the human failure snapshot; a machine
u64::MAX hint blocks fresh grants, other queued jobs and manual requests without
timestamp overflow. Waiting before execution consumes no attempt. The existing
manual service reports source_retry_wait distinctly from a rights/scope denial.
A shorter instruction cannot overwrite a longer remaining minimum. Only numeric
cooldown metadata, not publisher body or invented human actor, may be recorded by
a late machine response. The explicit real worker CLI is checked with no ready
work under restricted credentials, separate from injected HTTP interleavings.

Two restricted-role fixture failures were fixed before final verification: the
common insert initially listed human actor/notes columns even for NULL machine
values, and the manual pool lacked cooldown INSERT permission. Machine INSERT now
omits those columns entirely; the manual metadata grant is explicit. The full
twelve-suite wrapper was rerun after these changes; no permission was loosened to
let a worker fabricate a human snapshot actor.

Limits: trusted injected article transport exercises DB/time/race behavior; the
shared real HTTP parser/SSRF fixtures remain #18/#20 evidence. No real public-source
TLS/grant/license adjudication, deployed extraction crash/performance drill or
complete publishing journey is claimed. Grant time is fixed at one hour, lease at
60 seconds, transport at 20 seconds and automatic attempts at three. Unexpected
storage failures remain started until recovery; no successful response is invented.
Bounded orchestration is the next #21 step; protected UI is #22. Full-workspace CI
is separate. Live Claude Code runtime enforcement remains #3.

Rollback requires a matching-schema forward fix or verified restore; preserve
machine identity, grants/cooldowns and removal history. Do not invent human actors
for an old schema or silently restore publisher text after rights withdrawal.
