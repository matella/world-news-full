# Article-specific reporting origins

`OriginEvidence::assert` requires a live publisher session, source article ID,
expected origin version and `OriginInput`:

```json
{
  "state": "documented",
  "origin_ids": ["wire-service"],
  "public_basis": "Both outlets explicitly attribute the same dispatch.",
  "private_note": "Editor evidence retained privately; never rendered publicly."
}
```

Unknown evidence has no selected origins; documented evidence needs at least one;
conflicting evidence may list the competing origin groups. Lists are unique and
bounded to 20 enabled registry origins. Public basis is 1–2,000 characters and
private note 1–10,000. Registry hints never prefill a documented assertion. The
source may be disabled while its historical attribution is corrected; this action
performs no HTTP or extraction. Assertions retain captured names after renames.

Each assertion is immutable. Concurrent curators must reload after a version
conflict; a failed pointer change rolls back both assertion and labels. Lock order
is source, selected origins, then source article, with bounded transaction waits.
Intake versions do not change. Authenticated editors can read bounded history and
private details; this is a private service API, not an exposed web route.

## Frozen citations

Migration 15 adds `anchor_id`, an origin version/assertion, state, captured labels
and public basis to citations. A supplied anchor must exist as a claim in that
revision's body (including context/uncertainty children). An absent anchor remains
a general reference. Legacy claim identifiers are preserved separately and never
guessed into a body mapping. New citation snapshots are derived by the database;
caller-provided state/labels cannot forge them. A revision uses one origin version
per source article, and reviewed/published citation changes require a new revision.

`news.public_citations` retains its original columns and adds this frozen evidence
and `origins_current`. Private notes/actors are excluded. Currentness becomes false
after a newer assertion or origin disable; old published snapshots stay readable.
Two outlets sharing one documented origin count as two outlets and one origin,
never two independent reports. Unknown/conflicting states remain explicit. Reader
count definitions and complete publication acceptance remain follow-up work in #27.

## Authority and verification

The migration owner applies schema changes explicitly. Curators need SELECT on
assertions/labels/source metadata, INSERT on assertions/labels, assertion sequence
USAGE and column-level UPDATE on the article's two origin pointer fields, alongside
the existing auth/audit grants. The narrow `lock_reporting_origins_for_evidence`
helper is revoked from PUBLIC; grant EXECUTE to the curator role with the existing
`lock_source_for_fetch` helper. Do not grant registry UPDATE just to take share
locks. Reader roles continue to use public views; workers need no new authority.
The disposable restricted-role integration test is an executable grant example,
not permission to alter a live role.

Run `python3 scripts/test_database.py` after the locked Rust build. On small VFS
Docker workspaces, `WORLDNEWS_TEST_PG_TMPFS=1` puts this suite's disposable database
data on a capped 1 GiB tmpfs. This does not test disk persistence; use the separate
`scripts/test_persistence.py` drill for that. No existing container/volume is used.
See docs/evidence/article-origins.md and ADR 0017 for evidence and release limits.
