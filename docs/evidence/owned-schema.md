# Owned schema verification

Scope: #11, based on PR #52. R2: new isolated schema and an explicit DDL command.

Implemented two embedded, checksum-checked migrations in the reserved news schema.
Legacy/public objects are never altered. Added core records, immutable revisions
and publication events, edition snapshots, safe public views, bounded job metadata
and legacy import mappings. Application readiness validates schema versions and
checksums; web startup performs no DDL. Migration failures are sanitised at the CLI.

## Passed locally

- New crate unit/in-process tests: 9 passed. The opt-in DB suite is deliberately
  ignored by ordinary cargo test and is executed separately by the wrapper/CI.
- `python3 scripts/test_database.py` with local Cargo environment: one integration
  suite passed against a fresh uniquely named Postgres 18 Alpine database.
- The DB suite applied v1, upgraded to latest and reran migrations; preserved an
  unrelated public legacy sentinel; detected a changed migration checksum.
- Tested draft exclusion, same-story revision FK, append-only revisions/citations,
  matching publication events, job lease checks and metadata-only content policy.
- Tested transactional rollback, published edition immutability, correction without
  rewriting old membership, withdrawal notices/body exclusion and a restricted
  SELECT-only role denied access to internal tables and mutations.
- Wrapper also exercised the built migration CLI and a real web process returning
  readiness 200 with the restricted reader role after migration.
- Formatting, strict crate Clippy, whitespace/foundation/secret checks are recorded
  with the PR after the final run; no production verification is claimed.

An initial SQLx after-connect raw-SQL closure failed Rust lifetime checking. Session
settings now use typed PgConnectOptions startup options instead; compile and DB
tests passed. Tests use only synthetic text and credentials and remove only their
own container/anonymous volume.

## Limits and rollback

No live migration, publisher stop, role provisioning, merge or deployment occurred.
No authentication/editor workflow is claimed by the database constraints (#15/#16).
Individual JSON block/URL validation is a repository concern (#13/#26), and the
archive importer remains #12. Default-role grants are documented/tested, not applied
to the real host. Recovery policy is in ADR 0002: retain isolated data on app
rollback; restore verified backups separately, never auto-drop published history.
