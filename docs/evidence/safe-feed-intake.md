# Safe feed intake verification

Scope #18, stacked on registry PR #59. Internal safe HTTP/parser/poll services plus
additive migration 7. No executable polling loop, real feed setup or external fetch.

Passed locally:

- Strict affected-crate Clippy/fmt/build; 29 unit/in-process tests passed.
- Real loopback fixture requests with exact cfg(test) pinning: relative redirect,
  redirect loop, private literal redirect and public-looking redirect resolving to
  private socket; forbidden endpoint received zero requests. Mixed public/private
  DNS and cloud metadata/IPv4/IPv6 ranges rejected. No arbitrary DNS bypass flag.
- Both declared and chunked/no-length oversized responses rejected; compression,
  credential URL, exhausted request/DNS/parser slots and timeout exercised.
- ETag/Last-Modified sent only for their own URL, bound 304 accepted, another URL's
  validators not sent. HTTP 429 Retry-After parsed. GET -> RSS/Atom parser integration
  passed with no network to public sources or external entity/file endpoint.
- RSS/Atom retained metadata, relative links and dates; content/summary excluded.
  Malformed XML, external entity/DOCTYPE, deep nesting and UTF-16 fail closed.
- scripts/test_database.py: seven fresh DB suites passed (schema/import/public HTTP/
  auth/editorial/registry/feed polling), followed by real migration/import/auth and
  outage/shutdown checks. Registry projections stay compatible with new ops fields.
- Restricted fetch worker can poll but cannot change source enabled/config or write
  stories; reader cannot read batches or execute the source-lock function. No
  owner connection is used for application polling operations.
- Concurrent same-feed due calls produce one transport call; source-specific due
  reservation, 304 without batch loss, failure backoff, 429 delay and long-hint pause
  tested. Source config change during fetch cancels without advancing cache.
- Injected batch-storage failure leaves old cache/batches and unfinished started
  attempt, not fake completion. Retry stores batch/cache atomically. Durable metadata
  survives later 304. Captured context/completed attempts and batches cannot mutate.
- Fetching creates zero source articles/stories: normalisation and publication
  remain explicit downstream operations.

Initial HTTP fixture test caught reqwest rustls-no-provider requiring explicit
provider installation; constructor now safely installs Ring once when absent and
all fixture tests pass. Task-generated build artifacts were cleared to keep this
8 GB executor within bounds; source/data were unaffected. Debug-free local builds
are used; full-workspace CI runs on the standard runner.

Formatting, foundation/secret/diff guards and process checks passed before opening.
CI supplies full-workspace checks. New locked dependencies are
explained in ADR 0006; no security advisory audit is falsely claimed.

Not run/complete: real public-source/TLS/DNS/egress/proxy smoke, production grants,
source rights adjudication, normalisation, crash leases/worker recovery, admin UI or
new publishing journey. No source subscription or production state change occurred.
Explicit local HTTP fixtures and injected DB transport are distinguished from live
TLS verification. No provider costs, raw-body logging or external notifications.

Rollback after schema apply requires a matching binary/forward fix or verified
restore; no destructive down, history deletion or live migration is provided.
