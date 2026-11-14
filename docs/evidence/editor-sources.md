# Private source-selection evidence (#29 slice 3)

Passed locally on 2026-11-14:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all 19 disposable database suites and five Chromium flows passed against the final source/template/browser changes.
- `cargo clippy -p world-news --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `cargo build -p world-news --locked` and `python3 scripts/test_rust_runtime.py`.
- `python3 scripts/check_foundation.py`, `python3 scripts/check_deployment_policy.py`, `python3 scripts/check_secrets.py`, `git diff --check`.

The restricted-writer HTTP test creates a claim-based candidate, prepares references,
method/private proof and two attributed disagreement positions without saving, then
saves a new source-only revision. It checks immutable prose, captured version zero,
unpublished state, actual claim mappings, 400 recovery with escaped private input,
409 stale-save refusal, missing-CSRF/session denial and publication/actor spoof
rejection. It pages over 60 claims and 52 catalog records while retaining selections.
Changing a document with another document's pinned proof is rejected until explicit
refresh. Earlier sourcing/origin/public-projection regressions remain passing.

Chromium performs authoring followed by explicit source/method selection, document
pagination and return to the first page with selections retained, keyboard save,
private-note persistence, inert markup and mobile overflow checks. It makes no
external requests. An initial test exposed populated wrapped textarea labels;
explicit for/id associations fixed the source and article controls. Another initial
fixture assumption placed a reference on the first page; the browser now navigates
the real second catalog page. No assertion was skipped or weakened.

Self-review covered bounded selections/form fields, unknown proposal anchors,
explicit deletion/capture, sealed version retention, source-only review reset,
live reauthorisation on recovery and public/private field separation. Extremely
large packages beyond the bounded form cannot be edited through this screen;
existing stored data is preserved. No schema/dependency, HTTP publication or live
operation is added. This is not independent review or complete #29 acceptance.
Rollback removes the screens with a compatible reader/writer while retaining history.

GitHub workspace/frontend checks must pass before merge; unchanged frontend
checks are not run locally in this small workspace. Local fixture/cache settings are those documented in article-length and
editor-workbench evidence. No host Docker prune or production change occurred.
