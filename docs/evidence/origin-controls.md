# Private article-origin controls evidence (#29 slice 4)

Passed locally on 2026-11-14:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all 19 disposable database suites and six Chromium flows passed.
- `cargo fmt --all -- --check`, `cargo clippy -p world-news --all-targets --locked -- -D warnings` and `cargo build -p world-news --locked`.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`, `python3 scripts/check_deployment_policy.py`, `python3 scripts/check_secrets.py` and `git diff --check`.

The restricted-writer integration covers unknown version-zero heads, actual sealed
current evidence, publisher-only assertion forms, editor read/detail access,
anonymous denial, CSRF/Origin/role failures, unknown actor-field rejection,
nonpersistent selection paging, invalid-state recovery, literal markup escaping,
leading-newline preservation, immutable captured citation versions, new assertions,
stale-save refusal and oversized-selection recovery. Revoked identities cannot read
current evidence/choices or write assertions. Existing source, historical citation,
reader isolation, origin-disable and concurrent assertion regressions remain passing.

The new Chromium journey checks a fresh private justification, paged choices with
retained selections, keyboard sealed save, immutable private details/history,
nonexecuting markup, editor-role controls and mobile overflow. No external requests
are made. The new journey is wired into the existing disposable database runner;
no test or assertion was skipped or weakened. An initial compile found a test using
a crate-private CSRF accessor; it now obtains the editor token through the actual
served form without widening the authentication API.

The full DB/browser run preceded a wording-only refinement of conflict recovery,
which also covers lock contention rather than claiming every conflict means a new
assertion. Final unit/Clippy/build/runtime checks follow that wording change.
Self-review covered live permission checks at both HTTP/service boundaries, actual
head integrity, immutable proof, bounded/recoverable choices, fresh human notes,
private/public separation and no implicit citation refresh. This is not independent
review, full #29 acceptance or a live release. Registry/thread administration and
the complete editorial publication journey remain open. No schema/dependency,
provider, HTTP article-publication or production operation is introduced.

Rollback removes the controls with a compatible reader/writer while retaining
all sealed evidence. Local profile/cache/fixture settings are those recorded in
article-length/editor-workbench evidence; no host Docker prune occurred. Unchanged
frontend checks are not run locally in this small workspace; GitHub workspace and
frontend checks must pass before merge.
