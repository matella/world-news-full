# ADR 0011: Expiring publisher grants for machine extraction

Status: accepted for #21 slice 3, based on #64; not deployed.

A machine worker cannot use or fabricate a human session/actor. A live publisher
requests one article/version under current source rights and hostname scope; the
request is an audited grant valid for one hour. An active partial uniqueness key
and serialized grant requests prevent duplicate active work for the same intake
version. Cancellation needs publisher authority, expected job version and a reason.
New grants after terminal work require fresh live authorization and current policy.

Logout does not undo that explicit bounded delegation. Deactivating/demoting the
issuer, expiry, cancellation, source rights/version change or an intervening intake
edit prevents stale machine completion. A narrow fixed-search-path owner function
returns/locks only the issuer's active-publisher authority; workers receive no
identity/session SELECT. Lock order is issuer -> job -> source -> article. No lock
is held over the network. Human grant actions and worker attempts are separate.

Claims use 60-second generation-fenced ownership and at most three attempts.
Shared ArticleClient/SafeHttp keeps the 20-second transport/parser budget, same-origin
redirect policy, public DNS/IP/TLS rules and text bounds. Check grant/lease/context
before HTTP and again before storage. Text, source version/policy evidence, worker
snapshot, intake version/pointer, attempt and job success commit together. Failure
of acknowledgement cannot leave partial text. Lease recovery records abandonment,
not invented HTTP results; retry is bounded and permanent errors fail explicitly.

Snapshots now require exactly one origin: human actor_id or worker_job_id. A
composite FK ties worker snapshot to the job's article and each job has at most one
snapshot. Worker column-level INSERT excludes actor_id and notes; machine execution
cannot claim human authorship. Existing human rows remain valid. Private history
exposes both origin fields so consumers must label worker jobs explicitly. Source
storage revocation removes machine text through the existing policy trigger too.

Source-level extraction cooldowns store decimal Retry-After seconds plus observed
time, with provenance from a worker job or committed manual snapshot. Compare elapsed
numeric seconds rather than overflowing a timestamp. A shorter later hint cannot
shorten an active instruction. Manual and machine preflight share the same check;
a fresh grant cannot bypass a committed manual or machine source instruction.
A late worker can still record a real conservative cooldown after cancellation,
without authority to retain content. Human observations commit with the authorized
manual failure snapshot. Storage/process interruption can lose an uncommitted
observation; no impossible exactly-once HTTP/observation guarantee is claimed.

Transient failures retry with 10/20-second minimum delays and known short source
hints, within the three-attempt/hour grant. Longer hints stop automatic retry and
require review, while the source hold still blocks fresh/manual requests until it
elapses. Source-wait polling does not consume an unstarted job's attempt budget.
Grant expiry is checked even when input is waiting. No extraction is auto-enqueued
from a feed; each grant remains an explicit publisher decision.

Migration 12 is additive apart from making the snapshot actor nullable under a
strict XOR constraint. No dependency/service/provider is introduced. The explicit
--work-extraction-once command does not start a daemon, publish, create original
drafts, run an AI provider or subscribe to sources. Bounded orchestration remains
the next #21 slice, with protected UI in #22.

Rollback requires a matching-schema forward fix or verified restore. Preserve
worker provenance/grants/attempts and cooldown evidence; do not fabricate actors
for a downgrade or restore removed text silently. Live release remains #47–#49.
