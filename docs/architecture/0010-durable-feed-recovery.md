# ADR 0010: Recoverable feed attempts and atomic normalization handoff

Status: accepted for #21 slice 2, based on #63. Staged only, not deployed.

The existing feeds table already models scheduled work, configuration, cadence and
cache state; feed_fetch_attempts models attempts. Add lease/recovery there rather
than a second competing fetch queue. A due reservation records a unique attempt
with a 60-second lease and current source/feed versions. No later reservation can
replace an unclosed current attempt, even if next_fetch_at is manually advanced.
The shared HTTP/parser phase remains bounded at 20 seconds; no heartbeat is needed
inside that budget. A distinct attempt ID is the fence; IDs are never recycled.

Finish takes source -> feed -> attempt locks, requires a live started lease and
matching current pointer/configuration, and checks the original deadline again
before commit. A late/cancelled/recovered worker cannot advance cache. No locks are
held during HTTP. Source-specific next polling time is measured from completion,
with bounded exponential failure delays and a pause after five consecutive failures.
A reviewed publisher resume resets that failure cycle without shortening next due.

Successful modified batches, cache validators, completed attempt, a normalization
job and its immutable machine-origin link commit atomically. Origin references the
exact parent batch through a composite FK. No human operator event/actor is invented
for that enqueue. A later 304 creates no new job and cannot lose the previous batch.
Failure to create the job rolls back the batch/cache/completion too. Replaying the
normalization job uses #63's receipt and atomic acknowledgement.

Recovery scans at most 32 expired started attempts, then locks each in the same
source/feed/attempt order. Busy rows are skipped after a 250 ms lock budget; SQL
statements have a five-second budget. Current, still-enabled/configured attempts
become failed with lease_expired and preserve cache while applying cadence/backoff.
Disabled/changed or superseded attempts become cancelled; superseded history does
not rewrite the current feed. Repeated recovery is harmless. Recovery is bookkeeping,
not a fetch, subscription or automatic daemon.

Persist Retry-After seconds as bounded decimal text plus observed timestamp: a
u64-sized hint can exceed a representable timestamp. Numeric elapsed comparison
avoids overflow and never authorizes early resume/claim. Long hints keep the feed
paused. Upgrade cannot reconstruct hints that old workers discarded: historical
holds stay legacy_retry_instruction_unknown until independently verified. It does
not invent a safe deadline. Historic exhausted budgets are paused on upgrade.

Live-session publishers can pause/resume with expected operational control version
and private reason/audit. Pause closes in-flight started ownership; it cannot undo
a request already sent. Resume never performs HTTP and does not bypass source delay.
Editors can inspect bounded status/history without raw source URLs or feed bodies.
Registry configuration version is distinct from frequently changing control version.

Explicit one-shot CLI commands provide recovery and a selected feed fetch. Nothing
runs automatically with web startup. Real process-kill tests use a hanging trusted
test transport, never a production private-network escape. Normalization/HTTP fixture
boundaries stay separate. Machine extraction authorization and scheduler activation
remain #21 gates, with live release still requiring #47–#49 evidence/authorization.

Migration 11 is additive; no dependency/service changes. Rollback needs a matching
schema/forward fix or verified restore. Preserve attempt/job/source/cache history;
do not drop leases or remove conservative holds to make a rollout appear healthy.
