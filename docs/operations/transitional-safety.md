# Transitional deployment safety

Related issues: #5–#8. These are prepared source changes, not a live migration.

## Existing application configuration

`DATABASE_URL` is required (including local runs). Use a dedicated reader role for
the Next frontend. `NEWS_ADMIN_USER` and `NEWS_ADMIN_PASSWORD` enable the read-only
dashboard's transitional HTTP Basic protection; without both, admin fails closed.
Supply both through environment injection, never source. Terminate TLS at a trusted
proxy, configure request/rate limits there and do not publish Basic auth over HTTP.
This temporary mechanism is not the Rust editor/session system planned in #15.

Compose requires `WORLDNEWS_DB_PASSWORD` for bootstrap and `DATABASE_URL` with a
URL-encoded password. Example shape only: `postgresql://reader:<encoded>@postgres:5432/worldnews`.
Both host ports now bind loopback. A public site needs an explicitly configured
TLS proxy; do not merge without reviewing the actual host's routing. Never use
the database bootstrap superuser as the web reader in production.

An administrator can provision a reader with `CREATE ROLE`/a secret out-of-band,
then grant CONNECT on the database, USAGE on the schema and SELECT on
published_stories/news_status. The real schema must first be discovered. Revoke
CREATE/other unnecessary rights and configure publisher-owned default privileges
deliberately. Do not execute grants/role changes against production from this PR.

Database pool/query timeouts are bounded. Driver details/URLs are excluded from
application logs. Failed reads now reach a generic error page instead of empty
news. Next may stream a shell before a server error/404; callers requiring strict
pre-render HTTP semantics should use the new Rust routes once #14 lands. No claim
is made that a React error boundary can set an already-sent HTTP status to 503.

## Postgres durability

Compose already mounted the correct Postgres 18 parent `/var/lib/postgresql`.
Kubernetes now mounts that parent from a PVC and uses Recreate to avoid concurrent
writers on the same claim. It references the externally provisioned
`postgres-credentials` Secret, key `password`; API uses `world-news-app`, key
`database-url`. No secret values are committed. Select a StorageClass and reclaim
policy suitable for the cluster; PVC deletion is not a backup strategy.

**Do not apply the changed Deployment over existing emptyDir data.** First inspect
which workload is live, preserve a backup, restore it to an isolated volume, check
records/schema, and arrange an explicit writer freeze and migration window. For
Postgres 18 the default versioned data directory is under the parent mount; a
different `PGDATA` or old major version needs its own migration plan. Merely
changing mount paths does not move existing files.

## Backup and restore procedure

Using the authorised connection, `pg_dump --format=custom --no-owner` writes a
backup to access-controlled storage. Separately retain role/grant definitions
without passwords and any non-DB assets. Validate with `pg_restore --list` and
restore into a **fresh isolated** database using `--exit-on-error`; verify schema,
row counts, citations and sample edition/revision records. Never use `--clean`
against a live database as a routine restore test. Restore roles/ownership according
to least privilege rather than replaying every old grant.

Recommended initial policy: daily backups, 7 daily/4 weekly/3 monthly generations,
encrypted off-host storage and a monthly restoration drill. These are operational
targets, not configured or measured production guarantees. Set RPO/RTO only after
testing the real system. Do not purge any existing backups from this source change.

`python3 scripts/test_persistence.py` tests Postgres 18 parent mounts, recreation
and pg_dump/pg_restore with synthetic data and unique disposable volumes. It never
connects to a configured production URL. The full archive recovery exercise is #47.

## Reproducible checks

Node 24, explicit npm versions, npm ci, typecheck, ESLint and tests replace ignored
build failures. Next/React stay on their existing majors with current compatible
minor/patch versions to avoid preserving the obsolete 16.0.1 baseline. @types/pg
provides the missing driver types; ESLint and its Next config provide an actual
lint gate (Next 16 does not run lint automatically). The added driver types and lint tools use permissive upstream licenses; audit
the resolved tree as part of CI. ESLint 9 is a temporary compatibility exception:
the Next React/accessibility/import plugins still declare ESLint <=9 peers. Its
deprecation warning is recorded; remove this exception with Next retirement in #49
or a compatible plugin upgrade rather than installing unsupported peer combinations.
Rust is pinned to 1.91.1 with Cargo.lock and strict fmt/clippy/test/build checks.
The legacy Rust binaries still have no behaviour tests; this is explicitly a
compilation baseline, not proof of news-pipeline operation.

No existing deployment workflow is invoked by PR checks. Keep main unchanged
until the existing auto-deploy effects are reviewed. The canonical deployment
replacement remains #46; this batch does not claim Kubernetes rollout validation.
