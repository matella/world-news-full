# Transitional safety evidence

Scope: #5–#8, stacked on foundation PR #50. Risk R2: authentication/configuration,
CI and prepared storage manifests. This report does not authorise deployment.

## Passed locally

- Clean `npm ci --ignore-scripts --no-fund`; an earlier ordinary npm ci also passed.
- `npm run build`: production Next 16.4.6 build and TypeScript passed.
- `npm run typecheck`, `npm run lint`, `npm test`: passed, 4 focused tests.
- `npm audit --omit=dev --audit-level=high`: zero production vulnerabilities.
- Full audit: 7 moderate development-only transitive minimatch findings remain;
  no high/critical findings in the last full audit. `npm audit fix` did not resolve
  them within constraints. Do not call this a clean full audit. ESLint 9's temporary
  compatibility/deprecation exception is documented in the runbook and expires #49.
- `cargo fmt --all -- --check`, strict workspace Clippy, `cargo test --workspace
  --locked` and `cargo build --workspace --locked`: passed on Rust 1.91.1. The three
  old binaries have zero unit tests; no publishing correctness is claimed.
- `WORLDNEWS_TEST_POSTGRES_IMAGE=postgres:18-alpine python3 scripts/test_persistence.py`:
  persistence across recreation and pg_dump/restore integrity passed.
- `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_frontend.py`: passed against an
  isolated Postgres 18 Alpine DB with a SELECT-only role. Verified empty edition,
  invalid/missing dates, missing story, admin 401/authorised 200, normal read and DB
  outage. System Chromium confirmed a visible mobile error boundary/retry button.
- Compose validation with synthetic settings, foundation/GADS drift checks, narrow
  common-secret scan and diff checks passed. Secret scan is not exhaustive/history-aware.

Rust commands used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0` to reduce disk usage; CI may use its normal profiles.

## Failures corrected or bounded

Restored checks found missing pg types, stale npm metadata, unsupported TypeScript
configuration and link lint failures. Fixed rather than bypassed. Production
source-map-js and high PostCSS advisories were resolved. ESLint 10 was rejected
because current Next plugins declare <=9 peer support.

The 8 GB executor filled during Docker VFS copies and a subsequent verification
attempt. Only task-created caches/build outputs and task-pulled images were removed.
Tests now use one DB container at a time. Alpine passed locally; the default Debian
image and Kubernetes Pod recreation have not been verified here.

The first outage HTTP assertion expected boundary text in server HTML. Next sends
a streamed error signal and hydrates the boundary. The test distinguishes failure
from an empty edition and checks visible UI in Chromium. Strict 503 semantics are
reserved for Rust routes in #14; a React boundary cannot change sent headers.

## Release limits and rollback

Not merged/deployed. Real schema, secrets, ingress exposure and StorageClass remain
unknown. Never apply PVC changes over existing emptyDir data: preserve/restore first.
Loopback bindings and required configuration affect deployment and need review before
main merge. Provisioning live roles/secrets remains a separate authorised action.

Revert this source batch for code rollback. A live storage migration cannot be
reversed by reverting Git; recovery/cutover runbooks in #47/#49 govern that step.
