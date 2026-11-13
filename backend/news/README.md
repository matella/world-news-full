# Rust newspaper foundation

Related issue: #10. This is the runnable foundation, **not** the completed newspaper.
The current Next application remains the reader application until later issues land.

```sh
cargo build -p world-news --locked
DATABASE_URL=postgres://reader:fixture@127.0.0.1:5432/news cargo run -p world-news -- --check-config
DATABASE_URL=postgres://reader:fixture@127.0.0.1:5432/news cargo run -p world-news
```

The sample password is synthetic. Inject actual credentials outside Git. The config
check validates without connecting or calling providers. The binary defaults to
`127.0.0.1:3001` so it does not replace the existing app. Containers must explicitly
set `BIND_ADDR=0.0.0.0:3001`. The application handles SIGINT/SIGTERM and closes its
bounded pool after draining HTTP work. No Node, Jarvis or AI provider is needed.

## Configuration

| Variable | Behaviour |
| --- | --- |
| `DATABASE_URL` | Required Postgres host/database; never printed. Reader credentials only until mutation services exist. |
| `APP_ENV` | `development` (default) or `production`; rejects unknown modes. |
| `PUBLIC_URL` | Origin only, defaults `http://localhost:3001`; production requires explicit HTTPS origin. No credentials/path/query/fragment. |
| `BIND_ADDR` | IP:port, defaults loopback port 3001. |
| `NEWS_MASTHEAD` | 1–120 characters, template-escaped; defaults The World Brief. |
| `NEWS_EDITORIAL_CONFIG` | Optional JSON file path; otherwise compiles repository `config/editorial.json` as the default. |
| `NEWS_ADMIN_ENABLED` | `false` by default; `true` requires an explicit admin runtime connection and safe public origin. |
| `EDITOR_DATABASE_URL` | Private admin-runtime credential for the same DB target; no runtime schema/account ownership. |

Production database TLS uses hostname/certificate verification (`verify-full`).
An explicit weaker `sslmode` is rejected; configure the database trust chain before
production use. Local development can connect to a disposable Postgres without TLS.
Configuration validation sanitises failures and Debug omits connection options.
Statement logging is disabled to avoid exposing future editorial data.

Editorial settings validate schema/version, IANA timezone, word ranges and reading
budgets. Only manual publication/no AI and the initial French UI are implemented;
unimplemented modes fail validation instead of pretending to work. Settings are
not yet an editor or word-count enforcement; those are #28/#29.

## Modules and endpoints

- `domain`/`content`: validated editorial policy and typed revision/body models.
- `config`: typed startup settings with no provider/secret logging.
- `persistence`: bounded SQLx pool, explicit embedded migrations and schema readiness.
- `web`: Axum routes and escaped Askama HTML; shared by the executable and tests.
- `repository`/`error`: typed public/draft persistence and redacted failure classes;
  see [API contracts](../../docs/persistence-api.md).

`/health/live` is process liveness. `/health/ready` checks every migration version
and checksum via a restricted metadata view; absent/partial/drifted schemas return
503. Complete-pipeline freshness remains #44. Minimal HTML views now read published
editions/stories/archives from restricted views. Empty archives return 200; missing
public resources return 404; DB/schema outages return 503. `/recherche` and `/sujets`
return explicit 501 while their implementation is pending. Admin is disabled by
default; when explicitly configured, local identity/session/CSRF checks protect
its dashboard and logout. No publication endpoint or permissive CORS.
See the [HTTP contract](../../docs/http-contract.md) for validation, canonical URLs,
request IDs, timeout/body limits and what remains in the reader issues.

```sh
cargo test -p world-news --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/test_rust_runtime.py
```

The last command exercises the real built binary, startup validation, status
semantics and graceful shutdown without a live database or external provider.

## Dependency choices

Axum/Tokio/serde/tracing match the existing Rust stack. Askama 0.16.1 is the current
release inspected and supports Rust 1.88. SQLx 0.8.6 supports our pinned Rust 1.91.1;
SQLx 0.9.0 requires Rust 1.94, so it is intentionally not selected. Disable SQLx
default features: only Postgres and Tokio/Rustls are selected. Embedded migrations
and typed row derives use SQLx macros; chrono/json support typed data. chrono-tz validates the IANA timezone instead of
accepting an unchecked string. url provides maintained connection/origin parsing.
These packages publish under MIT/Apache-2.0 compatible licenses per crate metadata.
Cargo.lock pins transitive resolution. A RustSec audit is not claimed here;
dependency/security release verification remains part of #48.

## Owned schema and explicit migration

Read [ADR 0002](../../docs/architecture/0002-owned-database.md) before targeting a
database. Use `cargo run -p world-news -- --migrate` with an authorised migration
owner's DATABASE_URL. It changes only the reserved news namespace and never runs
automatically on web startup. Do not run against production without its explicit
migration/backup plan. Runtime readers need SELECT on news.public_* and
news.schema_version, not base tables or DDL rights. The legacy public schema is
untouched; import is a separate command/work item.

`python3 scripts/test_database.py` creates a uniquely named local Postgres 18
database, runs the deliberately opt-in Rust DB integration suite, and removes
only its own container/anonymous volume. It tests v1 -> latest migration, drift,
immutable revisions/snapshots, withdrawal, reader grants and draft isolation.

## Offline legacy import

`world-news --import-legacy FILE [--dry-run | --apply]` defaults to a bounded offline
plan, requiring no DB credentials. Apply is explicit, transactional and idempotent;
quarantine is reported with exit code 2 rather than silently dropping records.
Read the [format, mapping and staging runbook](../../docs/legacy-import.md) first.
This imports already-published history, not new intake or AI writing. SHA-256
fingerprints reuse the existing sha2 0.10 dependency (MIT/Apache-2.0); they establish
idempotency, not authenticity. JSON arbitrary precision preserves numeric values.
`http-body-util`, already locked for tests and used by Axum, is also a runtime
dependency for identifying actual streamed-body limit failures (MIT licensed).

## Local admin identity

Read [setup/session documentation](../../docs/admin-auth.md) and
[ADR 0003](../../docs/architecture/0003-local-admin.md). Explicit `--create-admin USER
editor|publisher` reads a chosen password from stdin; `--disable-admin USER` revokes
that identity's sessions. Neither runs at startup. Use an authorised migration/
bootstrap credential only for CLI operations, separate from runtime connections.
No real identities are provisioned by development/tests.

Argon2 0.6.0 and cookie 0.18.2 are current inspected Rust-compatible releases
(MIT/Apache-2.0); getrandom, subtle and zeroize reuse locked versions. Eighteen
transitive packages implement maintained hashing/PHC/cookie/time primitives. A
complete security audit, live TLS/proxy/grant verification and actual editorial
publication remain separate gates; the dashboard explicitly says so.

## Editorial service

The internal `EditorialService` now provides live-session-authorised create/save,
submit/review, publish/correct and withdraw transactions, plus bounded private
history. Actor IDs are never caller inputs. Candidate review state is independent
of public state so pending corrections preserve the approved article. Every write
requires the current lock version, and saved revisions invalidate approval.

See [service contracts](../../docs/editorial-service.md),
[ADR 0004](../../docs/architecture/0004-editorial-transactions.md) and the evidence
record. Citation/structure/word-budget checks and editor UI are still required;
no publishing HTTP endpoint is exposed yet. Migration 5 adds private review/action
state without inventing historical approvals or changing legacy public tables.

## Source registry

The internal `Registry` provides publisher-authorised, versioned source/feed/origin
configuration and editor reads. Unknown rights remain metadata/link only. Disabling
retains history; renaming an outlet preserves captured citation labels. Each change
has a private immutable before/after audit. Registry operations make no HTTP request
and start no subscriptions. See [contracts](../../docs/source-registry.md) and
[ADR 0005](../../docs/architecture/0005-source-registry.md). Admin screens and safe
fetching/extraction/jobs remain separate work; no new dependencies are introduced.

## Configured feed intake

Internal `SafeHttp`, `FeedClient` and `FeedPoller` implement bounded DNS-pinned
HTTP(S), RSS/Atom metadata parsing, per-feed due reservations/retry status and
immutable metadata batches committed with cache validators. No worker loop or
default subscriptions start with the web process. Fetched text is data, never an
instruction or a publishable story; content/summary fields are not retained.

Read [intake contracts](../../docs/feed-intake.md) and
[ADR 0006](../../docs/architecture/0006-safe-feed-intake.md). Production destinations
are public HTTP/80 or HTTPS/443 only, with TLS verification; local fixture permission
exists only in tests. No ambient proxy/credentials, automatic redirects/retry or
decompression bypass the shared policy. Normalisation, extraction, worker recovery
and protected operations UI remain separate issues. Tests use local fixtures and
private disposable metadata, not real feeds or owner credentials for runtime work.

## Source normalization

Internal `Normalizer::apply(attempt_id)` consumes durable metadata batches into
metadata-only source articles, immutable per-entry observations and replay receipts.
URL identity is conservative and versioned; headline/parser IDs never merge items.
Existing articles remain unchanged when new metadata differs. Missing/ambiguous
links and conflicting attribution quarantine for operator review. No article fetch,
origin certification, scheduler or story publication occurs.

See [normalization contracts](../../docs/source-normalization.md) and
[ADR 0007](../../docs/architecture/0007-conservative-normalization.md). Migration 8
is additive. `scripts/test_database.py` now includes a separate normalization DB
with concurrent, replay, rollback and restricted-worker checks.
