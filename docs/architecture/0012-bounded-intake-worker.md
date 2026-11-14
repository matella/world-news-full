# ADR 0012: Explicit bounded intake orchestration

Status: accepted for #21 slice 4, based on #65. Staged, not deployed.

Expose --work-intake-once as the supported bounded orchestration command. Do not
start jobs with web startup, automatically choose feeds, or grant extraction from
feed metadata. A strict typed policy has an explicit JSON list of at most 16 unique
opaque feed IDs, extraction disabled by default, normalization/extraction caps of
0–10 jobs and a 1–300-second work budget (default 60). Default work may recover/feed
bookkeeping and normalize existing queued metadata, but performs no source HTTP.
Preserve IDs exactly, including commas; CSV would ambiguously split valid IDs.

Run bounded feed lease recovery, each listed feed at most once, capped normalization
jobs and then capped explicitly authorized extractions only when enabled. Feed and
extraction primitives retain their own HTTP/parser/source/lease/rights safeguards.
Unexpected DB/config/permission failures stop later stages; a source fetch failure
is a measured result, not a fabricated completed pipeline. There is no broker,
provider or scheduler dependency.

A total async timeout cancels the in-flight stage and returns interrupted partial
counts with the active stage. Persisted started leases remain recoverable. Database
success acknowledgements retain the primitive transaction fences; timeout cannot
make partial content look published. A commit can happen just before its result is
observed: counts are observed results, job/attempt state remains authoritative.

The process runtime now explicitly caps shutdown waiting for blocking tasks at one
second after async application work returns. Native DNS/parser tasks may outlive a
cancelled future; they hold no content transaction. This avoids waiting indefinitely
for those tasks at CLI runtime drop. The work budget is not a hard wall-clock OS
execution guarantee: connection cleanup/committed queries can outlive cancellation.
A future reviewed host/container scheduler should enforce its own process deadline.
Web graceful shutdown still finishes the existing application future first.

Report only static stages/categories/counts, never feed IDs/URLs/text/credentials or
operator reasons. A completed feed check, normalized batch and private extracted
snapshot remain different stages, not story publication. Zero exit means the bounded
command observed no terminal failure/error/interruption; it does not certify empty
backlogs, all sources healthy, expired grants resolved or publishing acceptance.

Separate the worker's restricted DB identity from the public reader and authenticated
editor pool. It can combine the reviewed feed/normalization/extraction privileges;
it still cannot read sessions, invent grants/human actors, write stories or alter
source config. No new schema, dependency, process loop, scheduled deployment or
live credential/grant change is introduced. Deploy scheduling only with #46–#49's
reviewed image, identities, source allowlist, concurrency and release plan.
