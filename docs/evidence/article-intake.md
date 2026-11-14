# Article intake evidence (#20)

Staged on #61, not merged/deployed. Migration 9 changes app-owned tables only;
no live text/grants/config/accounts were changed. Public reader views gain no data.

Final local checks:

- `cargo test -p world-news --locked`: 33 unit/in-process tests passed; nine opt-in
  DB suites are ignored here and run separately with synthetic disposable resources.
- `cargo clippy -p world-news --locked --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `cargo build -p world-news --locked`: passed.
- `python3 scripts/test_database.py`: nine independent fresh databases passed
  schema/import/public HTTP/auth/editorial/registry/feed/normalization/intake checks,
  followed by real migration/import/account CLI and separated HTTP auth, truthful
  outage/readiness and graceful shutdown checks. No production data is copied.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. The secret guard
  is a narrow tracked-file check, not a full history/secret audit.

HTML/client fixtures verify paragraph order/entities, h1/author/date metadata,
active/hidden/aside removal, explicit paywall/JSON-LD denial, absent/ambiguous article
boundaries, oversized text and unsupported charset/media. Real local HTTP follows
same-origin redirects and rejects a changed origin before DNS/contact. A private
redirect endpoint records zero hits. Existing shared network tests retain DNS/IP,
TLS-constructor/size/timeout/conditional behavior; no real public TLS/source test.

Intake integration uses a restricted authenticated editor pool and trusted injected
article transport, separate from real HTTP fixtures. Unknown storage/wrong hostname
causes zero transport calls. Success stores private text/evidence while preserving
original article title/body/URL. HTTP denial becomes explicit failure with no text.
Manual metadata links require no feed/network; tracking repeats conflict. Notes plus
an independently authored initial draft link commit atomically. Injected link failure
leaves no draft/story/revision/note/version side effect. Drafts remain unpublished.
Stale intake versions conflict. Concurrent network phases commit only one snapshot.
Session revocation or source changes during fetch reject completion; no stale text
is retained. Rights revocation removes retained paragraphs, records the removal,
keeps authored notes and marks intake metadata-only. Restored permission does not
resurrect removed text. Reader cannot select private evidence/text/links; editor
cannot update original body/config or DDL. Snapshot/link rows cannot be rewritten.

Two interrupted verification attempts are not counted as passing: the linker
crashed with Bus error under the disk-limited executor, then an intermediate run
reached nine passing DB suites but failed final CLI migration checks because its
binary embedded an earlier revision of the staged migration. Only project-generated
Cargo artifacts were cleared, then the finalized migration/binary was rebuilt and
the entire wrapper rerun successfully. Final source/data remained intact.

Limits: UI (#22/#29), machine-worker identity/leases/retry (#21), claim-level source/
origin checks (#27), article lengths/editorial acceptance (#28/#29) and a complete
new-publishing journey remain separate. A service success is not publication. No
universal CSS/paywall detector, non-UTF-8 support, broad readability heuristic or
arbitrary CDN scope. No live provider calls, real licenses adjudicated or real
source subscriptions. No security advisory audit is claimed. New dependency/license
impact is documented in ADR 0008. Full-workspace checks run in CI; local builds use
the documented disk-limited profile. Claude Code runtime enforcement remains #3.

Rollback uses a reviewed matching-schema forward fix or verified backup/restore.
Never silently restore removed publisher text after storage revocation, discard
provenance or bypass schema checks. Rights-aware backup retention needs #47 review.
