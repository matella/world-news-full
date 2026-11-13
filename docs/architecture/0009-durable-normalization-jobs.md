# ADR 0009: Postgres leases and atomic normalization acknowledgements

Status: accepted for the first staged slice of #21, based on #62. Not deployed.

Use the existing Postgres database, not Redis/a broker. Initial jobs reference an
immutable feed batch, not arbitrary serialized instructions/URLs/body text. The
unique batch ID makes enqueue idempotent. Human publishers authorize enqueue,
replay and cancellation using live sessions, expected job version and a reason;
editors may inspect bounded private status/history. The worker has a distinct DB
role with no identity, story, config-write or operator-audit authority.

A job moves queued -> running -> succeeded, retry or failed; cancellation is
explicit. Claims use a transaction and FOR UPDATE SKIP LOCKED. Each claim increases
a generation fence and inserts an attempt; only that unexpired generation can
heartbeat/complete. Leases last 60 seconds. A claim call reaps at most 32 expired
attempts, records lease_expired and applies backoff or terminal failure. There are
1–5 attempts per authorized cycle, with delays of 10/20/40/80/160 seconds. A manual
replay resets the cycle budget while preserving all previous generations/audit.
Finished attempts, job identity/budget and operator events cannot be rewritten.

Normalization acquires the leased job row lock before its existing advisory/source
locks. It rechecks the deadline immediately before acknowledgement. New articles,
observations, receipt, successful attempt and succeeded job commit together. A
failed/expired acknowledgement rolls back the content transaction. The worker then
records a bounded retry if it still owns a valid lease; otherwise recovery is left
to the next claimant. An interrupted connection before commit leaves no partial
success; a committed success is visible as succeeded on restart, including replay
of an existing receipt. No exactly-once network delivery claim is made.

Cancellation/reclaim obtains the same job lock. It can win before handler entry,
fencing all new content writes, or observe the already committed result and refuse
to cancel completed work. The batch transaction retains that lock, so a second
worker cannot acquire the job while content is being written. Heartbeats are
available before/separately from handler entry; do not run a competing connection's
heartbeat while the handler owns the row lock. A transaction longer than its lease
rolls back at completion and needs a later retry. Performance budgets remain #45.

The first executable path is explicit --work-normalization-once. It does not fetch,
extract, discover jobs, publish, subscribe to sources or start a daemon. Fetch
attempt recovery/cache/cadence fencing and machine extraction authorization are
remaining #21 work; an editor session is not a perpetual worker credential. Do not
activate a scheduler until those remaining boundaries and release checks exist.
This slice proves a real idempotent handler, not completion of the whole queue issue.

Migration 10 is additive. No dependencies or new runtime services. Stop explicit
worker invocations for a rollout/rollback, then use a matching schema/forward fix
or verified restore. Never delete job/audit/batch history or reset old generations
as an implicit rollback. No live migration or role grants are authorized by this ADR.
