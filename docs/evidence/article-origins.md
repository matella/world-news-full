# Article-origin evidence (#19 / #27 first slice)

Migration 15, the private origin service, exact classification citation copying and
the new restricted-role integration suite implement article-specific attribution.
No dependency, source fetch, publishing endpoint or live schema/grant change.

Passed locally:

- `cargo fmt --all -- --check`, strict Clippy, locked build and 43 unit tests.
- Focused origin integration on a fresh disposable Postgres 18 database.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`:
  all 17 database suites, final real CLI/HTTP/auth checks and three Chromium flows.

The first full disk-backed attempt stopped at creation of the third fixture
database on a small VFS Docker workspace; it did not fail an application assertion.
The tmpfs option avoids that fixture storage limit. It does not replace the
previously recorded disk persistence/restore drill or certify production storage.

The new suite upgrades an already published citation from migration 14, preserving
original fields while explicitly marking unknown origins/no mapped anchor. It
checks syndicated two-outlet/one-origin fixtures, registry hints remaining unknown,
private note/actor exclusion, publisher/editor/worker separation, live revocation,
concurrent expected-version conflicts, atomic rollback, sealed immutable labels,
disabled origin rejection, forged-snapshot replacement, body-anchor validation,
mixed-version rejection and frozen classification copies after newer assertions.
Two fixture failures were corrected: the curator needed a narrow share-lock helper
instead of registry UPDATE, and classification's existing publication-event SELECT
grant was missing in the new test role. Application permissions were not broadened.

Self-review covers fixed-search-path/revoked-PUBLIC helpers, bounded input/history,
source/origin/article lock order, immutable old evidence and public projection.
It is not an independent review or automated truth verification. Complete claim
editing, count presentation, disagreements and publication checks remain #27;
that issue must stay open. Live release/rollback evidence remains #47–#49.
