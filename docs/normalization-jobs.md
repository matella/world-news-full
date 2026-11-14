# Durable normalization operations

This is the first service slice of #21. It operates only on existing durable
feed_batches. Feed attempt recovery/handoff is staged separately in
[feed operations](feed-recovery.md); machine extraction and scheduler activation remain pending.
There is no automatic startup work and no publishing authority.

## Operator API

JobAdmin::enqueue(session, batch_id, max_attempts, reason) requires a current
publisher. An existing batch job returns its original status/budget; it does not
silently replay or change its 1–5 attempt budget. Batch IDs reference preserved
input, never caller-provided task bodies. Invalid stored batches fail validation
in the normalizer and consume bounded attempts.

replay/cancel require current publisher authority, expected lock_version and a
nonempty reason (up to 2,000 characters). Replay is allowed only for succeeded,
failed or cancelled jobs; it resets the per-cycle attempts, retains generation
history and records the operator. Replaying succeeded work acknowledges the
existing immutable normalization receipt, without creating another article.
Cancellation fences queued/retry/running jobs. A committed success cannot be
cancelled retroactively. Stale role/session/version is rejected.

status, history and stats require a live editor/publisher session. History uses a
generation cursor and PageSize 1–100. Status includes counts, state, timestamps,
lease deadline and a fixed error category, not feed URLs/text. Stats expose state
counts and the age in seconds of the oldest ready job; future retries are not ready
backlog. Operator reasons are private audit data, not worker logs or public output.

## Explicit worker

Build with cargo build -p world-news --locked. With an authorized isolated worker
DATABASE_URL and normal environment policy configured, run:

    world-news --work-normalization-once

This claims at most one normalization job and exits. Output is JSON with status
idle, completed, deferred, failed or lost_lease, plus a job ID when applicable.
Terminal failure returns exit 2; unexpected DB/config errors return an error exit.
Idle means nothing was claimed, not that every job/pipeline stage is healthy:
recovery may have delayed or exhausted an abandoned lease. Completed means the
normalization transaction committed; quarantine observations may still need human
review. It never means article/story publication or whole-pipeline success.

Queue::claim, heartbeat and run_claimed support explicit process orchestration.
Lease fields are private; use the claim result rather than inventing a job actor.
All success writes are atomic with normalizer output. A worker without authority
cannot manufacture a human session. Network fetching is absent in this command.
The CLI is not a production cron/deployment recommendation; release remains #47–#49.

Leases are 60 seconds, checked against Postgres clock_timestamp. Heartbeat requires
an unexpired matching generation and cannot resurrect an abandoned claim. Recovery
checks at most 32 expired jobs per call and uses SKIP LOCKED; invoke again to drain
larger abandoned sets. Recovered work waits the same exponential retry delays
(10–160 seconds under the 1–5 attempt cap). Last-attempt expiry becomes failed.
Current attempts and old finished attempts remain distinct; only explicit replay
can reopen a terminal cycle. No timer/sleep is needed for correctness.

## DB role boundary

A normalization worker needs USAGE news; SELECT schema_version, feed_batches,
feed_fetch_attempts, source_articles, normalized_batches, source_observations,
normalization_jobs and normalization_attempts; SELECT(id,name) sources for captured
attribution; INSERT source_articles, normalized_batches, source_observations and
normalization_attempts; UPDATE job/attempt operational fields; EXECUTE
lock_source_for_fetch(text) and lock_feed_for_normalization(text). It does not need
job INSERT/sequences, config UPDATE, identity/session SELECT, operator action INSERT,
story/publication writes, private extracted text or DDL.

The authenticated JobAdmin pool keeps existing auth/session/audit grants and gains
SELECT normalization_jobs/attempts, INSERT normalization_jobs/job_actions, operational
UPDATE jobs/attempts, and USAGE normalization_jobs_id_seq/job_actions_id_seq. Publisher
checks happen again inside each transaction. Anonymous readers gain no grants.
All role testing uses synthetic disposable accounts; live grants remain unverified.

See ADR 0009 and the evidence record. Migration 10 adds job tables/constraints but
no daemon, host configuration, subscriptions, production access or second broker.
