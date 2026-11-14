# Durable feed operations (#21 slice 2)

FeedPoller now treats each due fetch as leased work. It still uses SafeHttp and
FeedClient; registration/resume/web startup do not fetch. The explicit CLI commands
below require a reviewed isolated worker DATABASE_URL and normal config:

    world-news --poll-feed-once FEED_ID
    world-news --recover-feed-leases-once

The first selects one configured feed, respecting enabled/paused/cadence/Retry-After
and the current started-attempt fence. It can make a real public HTTP request;
no default source is chosen or subscribed. It may recover an expired last attempt
and return recovered without fetching. Call again only when due. JSON reports fixed
status, attempt ID/count and safe error category; failed returns exit 2. Disabled,
paused, not_due, cancelled, lease_lost and recovered are distinct outcomes, not
publication/whole-pipeline success. The second command performs no HTTP; it closes
up to 32 expired attempts and returns the recovered count. Busy rows are skipped,
so zero does not prove no abandoned work. No scheduler/daemon starts automatically.

Claims have 60-second deadlines; network/parser work remains capped at 20 seconds.
Finish revalidates source/feed versions, enabled state, last-attempt ID, started
status and expiry while holding short DB locks, then rechecks the deadline before
commit. If storage/normalization enqueue fails, cache is unchanged and the attempt
remains started for explicit lease recovery. HTTP cannot be undone after dispatch;
operator pause/source disable prevents stale storage/cache completion.

Every modified batch commits its normalization job and normalization_job_origins
parent record before cache advancement. This is a machine event with no fabricated
operator actor. A 304 retains prior batches/jobs and adds only fetch history/cache
validation. An enqueue failure leaves no partial new batch/job/cache. Recovery
never deletes originals or invents a successful response. At most five consecutive
failures or abandonments are allowed before retry_budget_exhausted pauses the feed.
Normal successful checks reset the failure count. Source-specific retry delays use
the polling cadence, exponential backoff (capped at seven days) and source hints.

Retry-After <= seven days contributes to next due. Longer values pause for review;
the full decimal seconds and observation time are retained and checked before both
resume and claim. Even u64::MAX cannot overflow a timestamp into an early retry.
Resume cannot shorten an existing due time. A legacy paused hint without its value
is marked legacy_retry_instruction_unknown and cannot be resumed through this API.
Independent verification/reconciliation of those historical holds remains a live
upgrade gate, not an automatic deletion or override. Other historical pauses are
preserved, and old failure counts >=5 become exhausted holds.

FeedAdmin provides private live-session controls:

- status requires editor/publisher and returns operational version, last outcome,
  fixed error/pause reason, count, due time, current attempt/lease and retained numeric
  Retry-After metadata. No raw feed/article URL or response body is returned.
- pause requires publisher, expected control_version and a nonempty reason up to
  2,000 characters. It pauses future fetches and cancels current started ownership.
- resume requires publisher/version/reason, resets the reviewed failure cycle and
  unpauses only after any known source instruction elapsed. It performs no fetch.
- history is a private metadata page, id cursor and PageSize 1–100. Finished attempts,
  machine origins and operator audit are immutable; current status is operational.

The control version advances on claims/completions/recovery/operator changes, so
stale actions conflict. Cached publisher roles and revoked sessions are rejected
by shared transaction authorization. Source configuration versions remain separate.

Worker role additions to #18 grants: SELECT/INSERT normalization_jobs and
normalization_job_origins plus USAGE normalization_jobs_id_seq; UPDATE lease_until
on started fetch attempts; operational UPDATE fetch_control_version,
fetch_pause_reason,retry_after_seconds,retry_after_observed_at on feeds. Retain
existing source-share lock EXECUTE, attempts/cache/batch privileges. A combined
metadata worker can also receive #63 normalization privileges, as tested; it still
needs no identities, private extracted text, story/config writes or DDL. This does
not authorize machine extraction or operator-action insertion. Anonymous readers
gain nothing. FeedAdmin's authenticated pool needs feeds/attempt SELECT, source-lock
EXECUTE, operational feed UPDATE, attempt outcome/error/finish/lease UPDATE, INSERT
feed_operational_actions and USAGE its id sequence in addition to existing auth.
No live grants have been changed.

The old storage-failure test now explicitly recovers an expired attempt before a
fresh claim; resetting next due alone must not silently replace a live owner. SQL
fixture clock advances are test-only. The process kill test kills only its owned
child after a durable reservation, advances expiry deterministically, then uses the
real recovery CLI. No production egress/TLS/source policy or deployment is implied.

See ADR 0010 and the evidence record. Remaining #21 work: explicit machine extraction
authority/audit and bounded scheduler/orchestration. Protected UI is #22. Performance,
backup/restore and cutover remain #45–#49. Exact schema checks intentionally reject
an old binary after migration; use a reviewed matching-schema forward fix/restore,
not destructive audit deletion or silent lease resets.
