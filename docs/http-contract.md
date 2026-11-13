# Rust HTTP contract

Issue #14. Minimal escaped HTML establishes route/data/status behaviour; reader
design, translations, correction notices, accessibility audit and full parity stay
in #34–#40. This is not approval to replace the deployed reader.

| GET/HEAD path | Behaviour |
| --- | --- |
| `/` | Latest published edition in configured publication timezone, labelled by its actual date; 200 empty state if no edition exists |
| `/edition/YYYY-MM-DD` | Immutable ordered snapshot; invalid calendar date 400, absent published edition 404 |
| `/story/:id` | Current published revision; missing/draft/withdrawn 404 |
| `/story/:id?revision=:revision` | A publicly visible revision belonging to that story; otherwise 404. Snapshot links use this form, explicitly label retained revisions and link current version |
| `/archive` | Published dates, newest first, 20 per page; empty page 200 |
| `/archive?before_date=YYYY-MM-DD&before_id=:id` | Keyset pagination; both cursor fields required, invalid cursor 400 |
| `/recherche?q=...` | Query validated (maximum 200 characters); 501 search-in-preparation, never a fake empty result (#38) |
| `/sujets` | Explicit 501 topics-in-preparation (#39) |
| `/admin` and `/admin/*` | Disabled by default (503); explicitly configured sessions/roles protect admin views; see admin-auth.md |
| `/health/live` | Process liveness JSON, independent of DB |
| `/health/ready` | Exact migration versions/checksums via restricted metadata view; matching schema 200, otherwise 503 |

Every published-data request checks schema readiness before repository reads.
DB outage, permission misconfiguration or invalid stored content returns a redacted
503, with Retry-After 60. Unknown paths return 404. Public paths accept GET/HEAD;
other methods return 405 with Allow. Admin login/logout also accept POST under the
identity/CSRF contract; Allow reflects each route. HEAD responses have no body. No GraphQL or
internal HTTP API is introduced.

Public page canonical links derive only from configured PUBLIC_URL and constructed,
encoded route segments/cursor values, never Host/Forwarded. Read-only public routes
with trailing slashes redirect 308 to a relative slash-free path, preserving the
validated query. Unknown paths, admin paths and writes are not redirected. Date
format is strict; opaque IDs reject separators/control/whitespace and dot segments.
IDs are preserved, not lowercased or coerced into slugs.

Request paths and query strings are each bounded to 4 KiB. Malformed percent
encodings, invalid decoded path UTF-8, control characters, duplicate query names,
more than 32 query pairs, keys over 64 bytes or decoded values over 2 KiB return 400
(overall address overflow 414). Unrecognised bounded query values, including old
paper/density/font/masthead settings, are ignored and not reflected. Personalisation
is tracked separately; query input never changes server security or canonical host.

The outer middleware buffers at most 64 KiB of request body, including chunked
bodies and routes with no body extractor. Overflow returns 413; malformed body 400.
A 10-second budget covers body consumption and handler execution; expiry drops the
handler future and returns 503. SQL statements independently have a five-second
server timeout; dropping a Rust future is not claimed to instantly kill server SQL.
Future editor routes must explicitly review limits for their content contract.

Each response receives a server-generated time/sequence correlation ID. Incoming
X-Request-Id is ignored. Logs include only that ID, fixed route template, a bounded
method label, status and elapsed time. They exclude raw URI/query, opaque path
values, body, cookies, auth and forwarded headers. Proxy access logs need their own
privacy review in the deployment work. Request IDs are trace labels, not secrets.

All responses are no-store pending the correction/withdrawal cache policy. Error
responses are noindex. Content is template escaped; body blocks are only paragraphs
and h2/h3 headings; validated HTTP(S) source links never trigger server fetches.
CSP disables scripts, remote assets and framing; same-origin referrer policy and nosniff apply.
There is no CORS permission or cross-origin write mechanism. Authentication/CSRF
and identity audit are implemented in #15's branch; editorial writes remain unavailable.

The renderer shows original-language text and clearly labels unverified imported
archives. It does not yet implement French translation toggles, timeline/search,
reader design or complete public correction notices. A withdrawal removes story
bodies from all public revision routes and leaves only a withdrawn marker in the
edition. Deployment stays blocked on the complete acceptance/recovery/cutover gates.
