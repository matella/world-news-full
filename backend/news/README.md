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
503. Complete-pipeline freshness remains #44. `/` returns 503 with a preparation page;
it does not pretend an empty/new publishing implementation is finished. No admin,
legacy query or publication endpoint is implemented yet. No global permissive CORS.

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
