# Bounded intake worker

Build the Rust binary and provision a reviewed isolated worker DB role as described
in feed-recovery.md, normalization-jobs.md and extraction-jobs.md. Then explicitly run:

    world-news --work-intake-once

No worker starts with the web process. Defaults recover abandoned feed bookkeeping
and process up to five existing normalization jobs, with no feed HTTP or extraction.
Source fetches require the feed-ID allowlist. Extraction needs both opt-in and an
unexpired publisher grant; the worker cannot issue one itself.

| Environment | Default | Bounds/meaning |
| --- | --- | --- |
| NEWS_WORKER_FEEDS | [] | JSON list, <=16 unique exact opaque IDs; raw input <=64 KiB; no URLs |
| NEWS_WORKER_EXTRACTION_ENABLED | false | Exact true/false |
| NEWS_WORKER_MAX_NORMALIZATIONS | 5 | 0–10 primitive calls, stops at idle |
| NEWS_WORKER_MAX_EXTRACTIONS | 1 | 0–10 calls when enabled, stops at idle |
| NEWS_WORKER_BUDGET_SECONDS | 60 | 1–300 seconds of async stage work |

For example, set NEWS_WORKER_FEEDS to ["feed-one","feed-two"] using your environment
manager's literal JSON string mechanism. IDs are preserved exactly; commas inside
an ID are not separators. Empty strings, malformed JSON/booleans, duplicates, unsafe
IDs and out-of-range limits fail with static validation messages, without echoing
raw environment values. DATABASE_URL/PUBLIC_URL and TLS policy retain normal typed
configuration; no default credential, private-network escape or ambient proxy.

Execution order is feed recovery -> listed feeds -> normalization -> opt-in
extraction. Feed checks enforce source cadence/Retry-After/current ownership.
Modified batches atomically queue normalization; an explicit extraction grant is
still required afterward. A run invokes each listed feed at most once. Capped loops
and a total budget bound work; there is no endless polling/sleep loop. Unexpected
DB/permission errors stop later stages. Source HTTP failures and job retry/terminal
outcomes remain truthful separate counters.

JSON contains stages_finished, interrupted, active_stage, extraction_enabled,
recovered_feeds, per-stage counters and static errors. Calls counts invocations,
including an idle/skipped check, not HTTP requests or claims. Completed means the
primitive returned a completed stage, not publication; deferred/cancelled/lost leases
remain distinct. An interruption leaves its active stage and observed partial counts.
Job state is authoritative when a commit raced the report's observation.

Exit 2 means an observed terminal stage failure, DB-stage error or budget interruption.
Zero exit reports no immediate call error/interruption or Failed work result, and
does not prove every source is healthy or backlog empty: capped calls may
leave work; disabled extraction, source holds and unrelated failures can remain.
Idle/recovery-zero is not a global health assertion. Use protected status/history,
source attempts, job queues and later telemetry (#22/#44) for actual operations.

The runtime waits at most one second for blocking-task shutdown after the application
future ends. Database cleanup/OS scheduling can exceed the async work budget, so
configure a separate process deadline and avoid immediate process restart loops.
Retried source requests remain governed by persisted source minimums and leases.
A timeout can leave a started lease: recover it rather than resetting ownership or
pretending success. SIGTERM/web graceful shutdown retains its existing flow.

Scheduling is an explicit deployment decision, not a default feature toggle. For a
reviewed host/container scheduler, choose one worker identity, this one-shot command,
a concrete allowlist/opt-in policy, a non-overlapping schedule, a process deadline
and a verified image/schema. Do not activate a CronJob/cron entry during source
review or use production secrets/feeds in tests. #46–#49 own release and live rollout.
No scheduler manifest is silently applied by this change.

No story write, public text reuse, original draft creation, notification or AI provider
call occurs. See ADR 0012 and the evidence record. No new schema/dependency/service.
