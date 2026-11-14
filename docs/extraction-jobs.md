# Authorized extraction jobs

ExtractionAdmin::request(session, article_id, expected_intake_version, reason)
requires a live publisher, valid source storage rights/hostname scope and an elapsed
source cooldown. It grants one hour, records the real requester and captures exact
source/intake versions. A duplicate active request returns its existing grant,
without new attempts or silently extending authority. A new terminal-cycle request
needs fresh authorization. It is never automatically created from an article/feed.

ExtractionQueue uses a separate worker DB role and no human session. Claim/recovery
and run_claimed validate the issuer's current active publisher role, grant deadline,
lease generation, source rights/version, article version and origin scope. Logout
alone does not revoke an already-issued one-hour delegation. Role/deactivation,
manual cancellation and stale policy/content invalidate it. An intervening manual
note wins instead of being overwritten by machine evidence.

Run explicitly with reviewed isolated worker DATABASE_URL/config:

    world-news --work-extraction-once

It claims at most one job. JSON states idle/completed/deferred/failed/cancelled/
lost_lease include only a job ID when relevant. Failed exits 2; unexpected DB/config
errors are errors, never false completion. Idle means no supported work was claimed,
not that all source holds/grants are healthy. It can make a real public HTTP request
only under a grant; nothing starts with the web process. Tests of the real CLI run
with no ready work, separately from injected transport/HTTP fixture verification.

A 60-second lease covers the bounded 20-second shared article transport. Retryable
network/capacity failures have at most three attempts with 10/20-second minimum
waits and applicable short source hints. Permanent extraction/access/unsupported
errors and long retry instructions end the cycle. Success and final observed HTTP
failure produce private worker snapshots; crash exhaustion does not invent a
snapshot/response that was never observed. Unexpected storage errors leave started
ownership for explicit recovery. Recovery handles up to 32 expired jobs with
finished lease-expired history; a fresh process can retry within the same grant.

Human snapshot actor_id is nullable only when worker_job_id is present, and vice
versa. Consumers must display Worker job #ID, never invent an editor. Human grant
requester is available separately on the job. A job cannot write another article's
snapshot, notes or original story. Machine text is private untrusted evidence, not
an instruction, automatic original draft, claim citation or publication.

Source extraction Retry-After is shared by manual and queued work. Its record
identifies a worker job or committed human failure snapshot, never a forged actor.
Numeric seconds/time comparisons handle u64-sized hints without timestamp overflow;
only a longer remaining minimum replaces an active one. Fresh grants and manual
requests remain blocked until expiry. The queue postpones unstarted held work
without consuming an attempt; expired grants still cancel. No general cooldown
clear/override API is provided. Reviewed source instructions and backup retention
remain release obligations. Rights withdrawal removes retained worker text through
the existing source policy trigger; job/provenance metadata remain.

ExtractionAdmin status/history need a live editor/publisher. Cancellation requires
publisher/version/reason and fences an active attempt. History is a private bounded
generation page (PageSize 1–100), without response bodies/URLs. No extracted text,
raw transport errors, URLs, sessions or grant reason enters worker CLI/log output.

Role changes (local synthetic roles tested; no live provisioning):

- Worker: USAGE news; SELECT schema_version, extraction_jobs/attempts, source_articles,
  sources/source_origin_links for current rights, source_extraction_limits; SELECT(id)
  intake_snapshots for RETURNING; INSERT only snapshot columns source_article_id,
  version,source_version,worker_job_id,kind,error_category,policy,extraction_metadata;
  INSERT intake_text/extraction_attempts/source_extraction_limits; operational UPDATE
  extraction jobs/attempts/cooldowns; UPDATE only article intake_version,
  current_intake_id,extraction_state; USAGE intake_snapshots_id_seq; EXECUTE
  lock_source_for_fetch, lock_extraction_requester and extraction_retry_allowed.
- The authenticated admin pool retains manual/auth grants and gains job/attempt
  SELECT, job/action INSERT and job/action sequence USAGE, operational job/attempt
  UPDATE for cancellation, and source_extraction_limits SELECT/INSERT/UPDATE to
  retain manually observed instructions. Manual extraction needs EXECUTE
  extraction_retry_allowed too. Human role checks stay inside transactions.
- Anonymous reader gains nothing. Worker has no identity/session SELECT, grant/action
  INSERT, human snapshot actor/notes INSERT, original story/config writes or DDL.

Both authority helpers are fixed-search-path and revoked from PUBLIC. Grant context,
finished attempts, action/provenance rows are immutable. See ADR 0011 and the evidence
record. Bounded orchestration and protected UI remain #21/#22; no actual production
subscription, grant, provider invocation, migration or deployment occurred.
