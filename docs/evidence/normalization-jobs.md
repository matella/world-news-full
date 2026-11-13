# Durable normalization job evidence (#21, slice 1)

Based on #62; local/staged only, no merge, live migration/grant, deployment, daemon,
source subscription, fetch, extraction, publisher/provider call or story write.
No new dependency/service. Migration 10 adds app-owned job/attempt/operator tables.

Final local checks:

- `cargo test -p world-news --locked`: 34 unit/in-process tests passed; ten opt-in DB
  suites are ignored here and exercised by the disposable wrapper.
- `cargo clippy -p world-news --locked --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo build -p world-news --locked`: passed.
- `python3 scripts/test_database.py`: all ten separate fresh DB suites passed:
  schema, legacy import, public HTTP, auth, editorial, registry, feed, normalization,
  manual/extraction intake and jobs. Final migration/import/auth CLI and separated
  HTTP credentials/outage/readiness/graceful shutdown checks also passed.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. Secret guard is
  a narrow tracked-file pattern check, not a complete security/history audit.

Job assertions use a dedicated worker DB role with no human identity/session,
story/config writes or operator-action insertion rights. Synthetic human sessions
are used only in a separate operator pool. No fabricated permanent editor session.

Concurrent claim calls have one winner. Duplicate enqueue preserves initial job
budget and one audit event. Heartbeat requires a live lease; expired heartbeat
cannot resurrect it. A fresh Queue recovers an abandoned expired lease, records
lease_expired, observes backoff and claims a higher generation. The old claimant
cannot commit. Final-attempt expiry is terminal. Disabled input retries to a bounded
failure with a truthful failed result, not a falsely deferred/succeeded status.

Normalization receipt/article writes and success acknowledgement commit together.
Injected acknowledgement error leaves no new article/receipt and records retry.
An expiry injected inside the handler transaction prevents commit, then retries.
Cancellation before handler entry fences all new writes. Cancellation during a
blocked handler transaction waits on the same job row and rejects after success
commits, without undoing the completed work. Explicit replay of succeeded work
uses the same receipt and creates no duplicate article. Finished attempts/operator
events/job context cannot be rewritten or deleted.

The real --work-normalization-once process completes a queued job under restricted
worker credentials and emits only fixed status/job ID, without source URLs/passwords.
Operator stats and bounded history work; editor cannot enqueue, cached publisher
role changes and revoked sessions deny replay. Anonymous reader cannot select job
internals; worker cannot access identities, invent operator audit, write stories,
change sources or create schema objects. Zero stories are created.

An earlier run passed the preceding suites/service cases but failed the new CLI
process assertion: command dispatch had been placed in the legacy-import branch.
It was moved to the normal top-level command path, and the entire ten-suite wrapper
was rerun successfully. The final runtime/build checks use that corrected binary.

Limits: this is a real durable normalization handler, not completion of #21.
Remaining gates are fetch reservation/attempt recovery and fenced cache advancement,
machine extraction job authorization/current-rights capture, and explicit worker
scheduling/orchestration. No process-wide SIGKILL/real deployment crash drill or
production throughput/role test is claimed. Abandonment/deadline tests use controlled
DB timestamps and a reconstructed Queue; real CLI execution is checked separately.
Lease deadline is rechecked immediately before acknowledgement under a row lock;
Postgres transaction ordering, not a wall-clock exactly-once claim, is the guarantee.
Full-workspace checks run in CI. UI/reader/publishing acceptance and live Claude
Code instruction enforcement retain their own tracked gates.

Rollback: stop explicit workers; use matching-schema forward fix or verified
restore. Preserve batch, job, generation and audit history; no destructive down or
silent production replay. No main push/merge or live Jarvis action occurred.
