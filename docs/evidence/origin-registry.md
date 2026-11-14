# Private origin-registry evidence (#29 slice 5)

Passed locally on 2026-11-14:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all 19 disposable database suites and six Chromium flows passed.
- `cargo fmt --all -- --check`, `cargo clippy -p world-news --all-targets --locked -- -D warnings` and `cargo build -p world-news --locked`.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`, `python3 scripts/check_deployment_policy.py`, `python3 scripts/check_secrets.py` and `git diff --check`.

Restricted-writer integration covers explicit creation, private leading-newline and
literal-markup preservation, unavailable-by-default forms, validation recovery,
role/CSRF/Origin denial, unknown actor-field rejection, immutable identity/source
association, versioned retirement/private-note changes and stale-save refusal.
Private history retains old/new notes and the real actor, handles both typed Record
and raw-row disable snapshots, and pages by action ID. Reader pools and revoked
sessions cannot read the private history. Retired identities cannot support a new
article-origin assertion. Test-only curator grants are confined to the disposable
fixture; no live grants/accounts are provisioned.

The existing origin Chromium journey now creates and retires a registry identity,
verifies read-only identity controls, leading-newline note values, both audit notes,
inert markup, editor read-only controls and mobile overflow before exercising the
article-specific assertion journey. Registration performs no external requests or
article assertions. No tests or assertions were skipped or weakened.

Self-review covered service/HTTP permission fences, exact versions, immutable
identity, both stored audit shapes, explicit activation, private/public separation,
recovery reauthorisation and registry-versus-article semantics. The shared pipeline
textarea sentinel fixes leading newline loss without rewriting stored values; the
prepared proof-version label also remains truthful during stale-input recovery.
This is not independent review, complete #29 acceptance or live-release evidence.
Audited thread controls and the full publication journey remain open.

No schema/dependency, provider, HTTP article-publication or production change.
Rollback removes these controls while preserving registry/evidence/audit records.
Local profile/cache/fixture settings follow the earlier editor evidence; no host
Docker prune occurred. Unchanged frontend checks are not run locally in this small
workspace; GitHub workspace/frontend checks must pass before merge.
