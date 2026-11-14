# ADR 0003: local admin identity with private server-side sessions

Status: selected for implementation of #15; no real identity provisioning or live
security-boundary change. Stacked after the HTTP contract in PR #56.

## Context and decision

No trusted identity provider/proxy or production account directory is established.
Use local accounts, a maintained RustCrypto Argon2 implementation and opaque
server-side sessions. Do not reuse the transitional Next Basic-auth environment
variables, accept identity/role headers or assume access to a homelab SSO service.
Public reading stays anonymous; admin is disabled unless explicitly configured.

There are two application roles: editor can draft/request review; publisher can
also review/publish/correct/withdraw. Account administration is an explicit operator
CLI, not a publisher permission or an HTTP endpoint. Authentication is not the
editorial workflow: #16 enforces its transitions, actor identities and concurrency.

Runtime uses separate reader and writer credentials for the same database target.
Reader can select public views only. Admin runtime can read identity records,
maintain failed attempts/sessions and append audit events, but cannot create users,
change roles/password hashes or disable accounts. Migration/bootstrap credentials
are supplied only to explicit operator commands. Editorial base-table grants will
be specified separately with #16; no runtime connection needs schema ownership.

Argon2 0.6.0 supports Rust 1.85 (we pin 1.91.1), is the current release inspected
and is MIT/Apache-2.0. Its defaults are Argon2id v19, 19 MiB, two passes, one lane.
Hash salts come from the maintained library/OS RNG. Accept only those PHC algorithm,
version and cost parameters, so malformed stored records cannot request unbounded
hashing memory. Bootstrap requires a chosen passphrase of at least 16 characters,
at most 1,024 UTF-8 bytes. No default account/password is embedded or created.

Session/CSRF tokens use 32 independent OS-random bytes each, encoded as hex. Store
only SHA-256 token digests, not bearer values, in the private DB. An eight-hour
absolute expiry and revocation are checked on each request alongside the current
active user/role. Sign-in always creates fresh tokens. Disabling an account revokes
all sessions. Role changes take effect on subsequent requests; no cookie claims a
role. Domain transitions must revalidate permission in their transaction (#16),
including revocation races after an HTTP guard admitted a request.

## Browser and operational boundaries

HTTPS deployments use __Host- cookies, Secure, HttpOnly, SameSite=Strict, Path=/,
no Domain and Max-Age=28800. Both session and CSRF cookies are HttpOnly; the server
renders the CSRF value into protected forms. HTTP development is allowed only on
loopback public origins and uses separate unprefixed cookie names. No persistent
remember-me, external identity callback or user-controlled return URL is supported.

Every login POST requires the configured exact Origin; protected mutations require
that Origin plus a session-bound synchronizer value (form csrf or X-CSRF-Token).
Missing/null/cross-origin/duplicate Origin fails. Duplicate token cookies fail.
No CORS permission or forwarded-header authentication exists. Referrer-Policy is
same-origin: no external referrer, while Chromium retains same-origin form Origin.
The previously used no-referrer header produced Origin:null on form POST and was
incompatible with strict Origin enforcement; browser testing caught this.

Known accounts lock for ten minutes after five failures, persisted and serialised
in Postgres. Unknown/locked users still execute a same-cost dummy Argon2 operation
and receive the same failure. A per-process 30-attempt/minute window and two-worker
hash semaphore bound load; cancelling HTTP does not release a hash worker early.
This is a small-admin baseline, not a distributed perimeter rate limiter. Account
lockout can temporarily deny legitimate sign-in; retain operator recovery access.

Identity events append to private immutable audit records, with authenticated
actor IDs only when known. Failed logins never attribute an unauthenticated caller
as the user. Operator bootstrap/disable records a subject with no invented actor.
Never record credentials, tokens, password hashes, raw forms or proxy headers.

## Dependencies, validation and rollback

cookie 0.18.2 (MIT/Apache-2.0) handles parsing/serialization; its time dependency and
Argon2's crypto/PHC dependencies add 18 locked packages. getrandom, subtle and zeroize
reuse existing resolved versions. Token generation, timing-safe comparison and
password hashing use maintained libraries; no custom cipher or signed-cookie
protocol is introduced. A comprehensive RustSec/supply-chain audit remains #48.

Validate with synthetic accounts, actual restricted DB roles, anonymous/public
reads, escalation attempts, lockout, expiry, CSRF, logout/revocation, failure
mapping and a real browser. Do not create real accounts as a side effect of tests.
TLS termination, proxy exposure and actual production grants remain release gates.

Before schema apply, revert this source branch to roll back. After an authorised
v4 migration, older exact-version binaries fail readiness: retain a v4-compatible
binary/forward fix or use a separately verified backup/restore plan. No automatic
DROP/down migration or removal of identity/audit history is included.
