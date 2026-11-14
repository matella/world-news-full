# ADR 0006: bounded direct fetches and durable metadata before cache advancement

Status: selected for #18. Explicit configured-feed service, no startup polling loop,
real subscription, provider request or automatic article publication.

Use one shared SafeHttp boundary for RSS/Atom and future permitted extraction.
Resolve each URL/hop, reject any nonpublic answer (including mixed public/private
sets), then pin the validated addresses while retaining the hostname for TLS/SNI.
Allow only HTTP/80 and HTTPS/443 in production. Reject userinfo/known credential
query parameters, obvious local names, special-use IP ranges and HTTPS downgrade.
Revalidate every redirect; maximum three hops, loop detection, no automatic retry,
redirect, cookie jar, decompression or ambient proxy/credential forwarding.

Direct DNS pinning cannot safely inherit an arbitrary HTTP_PROXY: that could move
resolution/connect authority elsewhere. Product proxy support needs a separately
reviewed policy-aware adapter. This is a product egress choice, not an executor
network-policy workaround. Tests make only loopback requests through a cfg(test)
fixture capability for one exact fake hostname/socket; it is absent in production.
Normal development/package network commands retain the environment proxy/CA trust.

Per shared client, bound logical requests to four, native DNS workers to two and
all-hop request time to fifteen seconds (three-second connect budget). DNS workers
retain their permit if their future is cancelled until OS resolution actually
finishes. Bound/validate all returned addresses (maximum sixteen); never fall back
to unvalidated DNS. Rustls uses platform certificate/hostname verification and the
maintained Ring provider; constructor installs it once if none exists. No insecure
certificate path is provided. Request bodies/auth headers are never sent.

Conditional validators belong to the exact fetched URL. A redirect to another URL
must not receive another origin's ETag/Last-Modified. Accept 304 only after sending
known validators, preserve missing 304 validators and keep its meaning distinct
from an empty 200 feed. Stream response bytes with a hard configured cap, including
unknown/chunked Content-Length; refuse compressed responses rather than expand an
unbounded payload. Send an identifying client/site User-Agent and no sensitive logs.

Parse using feed-rs 3 with quick-xml preflight: no DTD/external entities, NUL/UTF-16,
more than sixty-four levels or 100,000 XML events. RSS/Atom only in this version.
At most 500 entries, bounded metadata fields and two blocking parser workers, whose
permits also survive cancellation. Parser deadline is three seconds; poll wrapper
has a twenty-second outer budget. Reject overflows, never silently truncate.
Only title/link/time/language/author-name metadata is retained. No article body,
summary or author email is stored, rendered as markup or sent to an AI provider.

Feed IDs/timestamps are parser-normalised: missing IDs can be generated and RSS
updated can be copied from published. Label that provenance; #19 must not treat a
parser fallback as an original publisher GUID or irreversibly merge by title.
Source data is not a story, reporting-origin verification or permission grant.

Reserve a due feed atomically against source/feed enabled state and next_fetch_at;
commit before network work. A second worker observes NotDue. Persist a started
attempt, then recheck/lock source configuration before feed completion. Discard
results if versions/enabled state changed. The worker has no authority to change
registry configuration or create/publish stories. A narrowly scoped SECURITY DEFINER
function holds the source share lock without granting UPDATE sources; it has a
fixed search_path, fully qualified table and explicit REVOKE PUBLIC/worker EXECUTE.

Successful metadata batch, semantic hash, terminal attempt, validators and next
poll commit atomically. This is essential: advancing an ETag without durable entries
would lose work on a crash followed by 304. Batches are immutable/private and ready
for #19 replay. Completed attempt/context cannot be rewritten. A failed completion
transaction retains old cache and an unfinished started attempt, not fake success.

Configured cadence is 60–86,400 seconds. Failures back off exponentially (up to a
week) without polling faster than configured. Honour numeric/HTTP-date Retry-After;
hints beyond one week pause automatic fetch pending operator review. Paused is not
a promised retry ETA. Status/attempts contain fixed error categories/counts/times,
not raw request or response text. This is fetch status, not whole-pipeline success.

Crash leases, abandoned-work recovery and audited manual retry belong to #21; there
is no background worker/startup network side effect yet. A crash after reservation
can leave started until a later due poll/reconciliation. Do not claim job recovery
or a usable ingest UI before #19–#22 and the complete acceptance evidence.

reqwest 0.13.5 (MIT/Apache-2.0, Rust 1.85), feed-rs 3 (MIT) and quick-xml 0.42
(MIT, Rust 1.86) compile with pinned 1.91.1. Disable reqwest defaults and choose
Rustls/Ring without AWS-LC; ipnet/httpdate reuse existing versions. Twenty-four
additional resolved packages provide HTTP/TLS/parser/platform support. A full
RustSec audit remains #48. No Node/provider runtime dependency is introduced.

Migration 7 is additive/private. Before apply revert source; afterwards retain a
matching binary/forward fix or verified restore. No automatic destructive down,
feed/citation deletion, production role provisioning or live migration occurs.
