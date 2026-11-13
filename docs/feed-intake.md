# Configured RSS/Atom intake contract

Issue #18, architecture ADR 0006. These are internal Rust services; running the web
binary does not start polling. Configure sources/feeds explicitly before worker
integration. No production feeds, local-source escape flag or provider calls ship
as defaults. Public reading remains independent of intake availability.

## APIs and outcomes

- SafeHttp::get(url,max_bytes,validators) is the common outbound boundary. It
  returns a bounded body with final URL/validators or a distinct 304/error.
- FeedClient::fetch adds bounded RSS/Atom metadata parsing. Entries are untrusted
  data: no embedded instructions execute, and nothing creates a story/draft.
- FeedPoller::poll_due(feed_id) reserves an enabled, unpaused due feed and records
  its attempt. Outcomes: Disabled, Paused, NotDue, Cancelled, Unchanged, Modified
  or Failed, plus LeaseLost/Recovered for expired ownership. They are distinct from DB/config errors, which return AppError.
- FeedPoller::new uses the real shared client. with_transport is trusted Rust
  dependency injection for fixtures/policy-aware adapters, never a user input or
  HTTP option that disables destination checks.

Source/feed must both be enabled; next_fetch_at controls cadence. A configured feed
is not proof of current freshness. Network work holds no database transaction.
Recheck configuration before completion; changed/disabled work does not advance
cache or save a batch. In-flight GET may finish before cancellation is observed.

Modified responses persist an immutable private feed_batches record, including
source/feed config versions, resolved feed URL, parser format, metadata entries,
byte count and semantic SHA-256. Its cache/status, normalization job and immutable
machine-parent link commit in the same transaction.
304 records an observed successful check and retains previous durable batches;
it does not mean an empty feed. Hashes detect metadata changes, not authenticity.
No source article deduplication, extraction, edition or public story is performed.

## Bounds and destination policy

| Area | Limit/behaviour |
| --- | --- |
| Schemes/ports | HTTP 80 or HTTPS 443; no userinfo/known credential query fields or HTTPS downgrade |
| Resolution | Reject local/special-use/public+private mixed answers; pin <=16 validated sockets per hop |
| Redirects | At most 3, with loop detection and fresh validation/DNS pinning each time |
| Network | 4 logical requests, 2 native DNS workers per shared client; 15s total, 3s connect |
| Bodies | 16 KiB–10 MiB configured cap, 2 MiB default; checked while streaming, not just Content-Length |
| Encodings | Identity only, compression disabled even under feature unification; no UTF-16/NUL XML |
| XML | No DTD/external entity behaviour; <=64 levels/100,000 events/500 entries |
| Parser | 2 blocking workers; 3s parser timeout; entire poll transport <=20s |
| Metadata | IDs/links <=2,048 bytes, titles <=4,096, <=20 links/authors per item, author names <=500 bytes; batch JSON <=4 MB |
| Poll/retry | Configured 60–86,400s cadence; bounded exponential failure backoff <=7 days; Retry-After never accelerates a poll |

Longer Retry-After pauses automation for review instead of ignoring the server and
polling early. fetch_paused means there is no automatic next attempt, regardless
of the stored reservation time. #21's staged feed recovery now implements audited
pause/resume, five-failure cycle exhaustion, 60-second leases and explicit bounded
abandoned-attempt recovery. A failed storage completion leaves started rather than
confirmed success. See docs/feed-recovery.md for exact recovery/Retry-After and
historical unknown-hint holds; no startup worker is implied.

DNS and parser blocking permits remain occupied until their real work ends if a
caller times out; cancellation does not create unbounded detached workers. Product
fetches use direct pinned sockets and never ambient proxy credentials. Environments
requiring an egress proxy need a reviewed adapter, not an implicit bypass of policy.
TLS certificate/hostname validation remains enabled with platform CA trust.

The only local-address permit is a cfg(test) fixture constructor for an explicit
fake hostname and loopback socket. Production has no global allow-private flag.
Tests cover literal and DNS/redirect private targets, IPv4/IPv6 metadata networks,
mixed answers, streaming overflow, conditional URL binding, timeouts and capacity.
No real external source fetch is part of executor validation.

## Metadata provenance and rights

feed-rs can synthesise an ID from a link/UUID; parser_id is labelled accordingly.
RSS updated may be normalised from published. Neither is certified original GUID/
original timestamp. Links are resolved against the fetched base; retain that base
and parser format in provenance. The original byte stream/full text is not archived.

Title/time/link/language/author-name fields are kept as data; HTML-like title strings
must be escaped by consumers. Drop content/summary and email fields. Source rights
remain explicit registry decisions; a feed is not copyright permission and is not
independently verified reporting. #19 normalises/quarantines candidates, #20 performs
rights-aware extraction/manual intake. No automatic republication or AI call.

## Private worker grants and integration

The fetch worker needs USAGE news; SELECT schema_version/feeds/fetch_attempts/batches;
SELECT(id,enabled,lock_version) sources; UPDATE only feed operational/cache columns;
INSERT attempts/batches; UPDATE terminal attempt fields; USAGE attempt sequence;
EXECUTE lock_source_for_fetch(text). That function is REVOKEd from PUBLIC and does
not return notes or grant config UPDATE. No auth account, story, publication, DDL,
registry configuration or audit-rewrite permission is needed. Test roles prove it;
actual provisioning is an explicit environment release operation. The #21 feed
recovery slice also needs normalization job/origin INSERT/SELECT, job sequence and
lease/control/Retry-After operational fields; see docs/feed-recovery.md.

All observed status stays private until protected admin views (#22). Errors are
safe categories, never driver/parser/request text. Normaliser (#19) can replay a
batch after crash even if the next HTTP request is 304. Job scheduling/leases/retry
and pipeline UI retain remaining #21/#22 gates. The staged one-shot polling and
recovery CLI is explicit; no background loop is wired into the web process.

Build, run cargo tests and scripts/test_database.py. The wrapper now creates eleven
fresh local databases including staged feed/job recovery, and exercises real binaries
separately. See the evidence
record for exact outcomes and limits. Release still requires recovery, real TLS/
egress/grant validation and the complete manual publishing acceptance journey.
