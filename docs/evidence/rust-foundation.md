# Rust application foundation evidence

Scope: #10, stacked on PR #51. Risk R2: explicit dependencies, new isolated app.
No existing reader routes, publisher or deployment is replaced.

## Passed locally

- `cargo test -p world-news --locked`: 9 configuration/domain/HTTP tests passed.
- `cargo test --workspace --locked`: new tests plus legacy skeleton build passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo build --workspace --locked`: passed.
- `python3 scripts/test_rust_runtime.py`: built binary validates configuration,
  redacts secrets, serves liveness independently of DB, returns readiness 503 for
  unreachable DB, renders explicit preparation page and shuts down on SIGTERM.
- Foundation/GADS checks, common-secret guard and diff inspection/check passed.

Local Rust builds used debug=0/incremental=0 environment overrides to conserve
disk. No Node command, Jarvis call or AI request is involved in Rust verification.

## Dependency review

Crate metadata inspected: Askama 0.16.1 (Rust >=1.88, MIT/Apache-2.0), SQLx 0.8.6
(MIT/Apache-2.0), chrono-tz 0.10.4 (Rust >=1.65, MIT/Apache-2.0). SQLx 0.9.0
requires Rust 1.94, incompatible with pinned 1.91.1; 0.8.6 is deliberately pinned.
SQLx only enables Postgres/Tokio-Rustls. Existing Axum/Tokio/serde/tracing choices
are reused. Lock resolution succeeded and actual compilation establishes local
compatibility. A RustSec audit was not run; no vulnerability-free claim is made.

## Remote evidence for preceding batches

Foundation #50: https://github.com/matella/world-news-full/actions/runs/43418944270

Safety #51:
- Foundation audit: https://github.com/matella/world-news-full/actions/runs/43421614468
- Application checks: https://github.com/matella/world-news-full/actions/runs/43421614824

Both preceding workflows succeeded, including CI's default Postgres 18 persistence
drill. This adds remote evidence beyond the locally tested Alpine variant. No
Kubernetes migration or production backup is implied.

## Limits and rollback

The new root page deliberately returns 503/preparation; no article querying,
editing, ingestion, authentication or publishing is implemented in this binary.
DB readiness is a connectivity check, not migration or pipeline readiness. Typed
word budgets do not yet enforce writing length in an editor. These are separate
issues and must not be closed by this scaffold. It is not release-ready.

Revert this commit to remove the isolated crate/dependencies; there are no schema
or data mutations. Main and the current app remain untouched. Next work starts
with owned schema #11, then legacy import #12 and typed repositories #13.
