# Private authoring evidence (#29 slice 2)

Passed locally on 2026-11-14:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all 19 disposable database suites and five Chromium flows passed.
- `cargo fmt --all -- --check`, `cargo clippy -p world-news --all-targets --locked -- -D warnings`, `cargo build -p world-news --locked`.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`, `python3 scripts/check_deployment_policy.py`, `python3 scripts/check_secrets.py`, `git diff --check`.

HTTP integration verifies preview without mutation, private draft creation, public
404, inert markup, CSRF/session denial, stale-edit recovery, forbidden publication
intent/actor spoof fields, scoped large-form limits, malformed encoding rejection,
literal replacement-character acceptance and the legacy ID `new`. Title-only edits
retain leading newlines, original CRLF and decomposed Unicode in stored prose.
Unit cases cover block roundtrip, explicit deletion, stable claim IDs and repeated
section rejection. Chromium exercises keyboard add/preview/save/title correction,
claim identity, reader privacy and mobile overflow. No external requests are made.

The final broad database/browser run preceded a conflict-message wording change;
unit/Clippy/build/runtime guards passed afterwards. No assertion was skipped or
weakened. Local cache/profile constraints are those recorded in article-length
and editor-workbench evidence. No frontend files or dependencies changed; GitHub
workspace/frontend gates must pass before merge.

Self-review covered form bounds/encoding, no silent text loss, revision fencing,
recovery reauthorisation, historical identity, private sourcing reset and shared
escaped preview. This is not independent review, full #29 acceptance or live-release
evidence. No HTTP publication, schema or production operation is introduced.
Rollback removes these routes with a compatible reader/writer; preserve revisions.
