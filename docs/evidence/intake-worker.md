# Bounded intake worker evidence (#21 slice 4)

Based on #65; staged only. No new migration/dependency/service, live account/grant,
source subscription, scheduler, deployment, merge, Jarvis action or provider call.
The web process still starts no worker. Default policy performs no source HTTP.

Final local checks:

- `cargo test -p world-news --locked`: 36 unit/in-process tests passed; thirteen
  opt-in DB suites run separately using synthetic isolated local databases.
- `cargo clippy -p world-news --locked --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo build -p world-news --locked`: passed.
- `python3 scripts/test_database.py`: all thirteen separate DB suites passed,
  including preceding twelve and the orchestration suite. Final migration/import/
  account CLI, separated auth/HTTP/outage/readiness and graceful shutdown passed.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. Secret guard is a
  narrow tracked-file pattern check, not a complete security/history audit.

Policy tests verify no default feeds/extraction, bounded caps/budget, exact unique
opaque IDs, malformed JSON/booleans/numeric limits, duplicates and resource limits.
JSON preserves commas inside IDs; review replaced an ambiguous initial CSV design.

The orchestration DB test uses a combined restricted metadata/extraction worker DB
identity and real publisher session only to issue a separate extraction grant:

- One allowed feed -> committed batch/job -> normalized source article in one run.
- A pending authorized extraction remains untouched under default policy; no feed
  or article transport call occurs. Explicit extraction opt-in then stores worker
  provenance rather than a human actor. No original draft/story is created.
- A cap of one normalization leaves the other job queued; the next run completes it.
- Budget cancellation during a hanging feed leaves started leased ownership and
  reports interrupted/active stage, without successful content/cache. Explicit
  expiry/recovery followed by a new allowed run succeeds.
- The real --work-intake-once CLI defaults produce no feed calls/extraction and
  redact source URLs/credentials. A real CLI budget interruption during delayed
  normalization SQL exits 2 with partial counts and leaves recoverable running
  ownership rather than false completed content.
- Reader-only DB credentials cause a fixed forbidden recovery error before any
  network stage. Worker cannot read identities, create stories or change sources.
  Existing primitive suites continue to verify lease/source/actor transaction fences.

Blocking runtime shutdown now uses an explicit one-second wait after the async
application future ends; existing real-process SIGTERM behavior still passes.
Work budgets do not claim hard OS wall-clock deadlines or instant DB cancellation.
Counts are observed results, not authoritative global health/backlog snapshots.
A commit that raced reporting remains visible through durable job/attempt state.
No deployed process/native-DNS hang or production throughput test is claimed.

Initial strict compilation caught unused fixture imports; those were removed before
verification. The full thirteen-suite wrapper was rerun after the exact-ID JSON
policy change. Final code includes that policy and corrected fixtures.

Remaining: protected pipeline UI #22, full editorial/publishing acceptance, operational
telemetry/performance/recovery/deployment and live release evidence #44–#49. The #21
core queue/authority/orchestration paths are staged across the linked PRs, not merged or
activated. Full-workspace CI is separate; Claude Code runtime enforcement remains #3.
Rollback stops explicit worker invocations and retains durable jobs/grants/attempts;
no destructive audit reset, secret/permission change or automatic scheduler is shipped.
