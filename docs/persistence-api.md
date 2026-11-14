# Typed persistence API

Related issue: #13. Repository shares a bounded SQLx pool; no internal HTTP API
or presentation/theme logic is introduced. Readers use only public_* views.

| Method | Contract |
| --- | --- |
| story(id), revision(id) | Typed visible revision or NotFound; never returns a draft |
| editions(zone,cursor,size) | Explicit timezone and keyset date/ID pagination, 1–100 rows |
| latest_edition(zone) | None means a successfully queried empty archive; failures remain errors |
| edition(date,zone) | Published edition metadata or NotFound |
| edition_entries(id) | Exact snapshot order, optional withdrawn body, maximum 200; overflow errors instead of truncation |
| sources(revision_id) | Validated HTTP(S) links without credentials; bounded/deduplicated |
| draft_head(id), create_draft(input) | Privileged persistence only; read role denied; no public write endpoint |

All caller values are bound parameters. IDs, language-tag syntax, timezone, body
block shapes/headings/total size and enums are validated. Malformed stored rows
produce InvalidData, not empty content or a panic. Paragraphs are plain text;
templates must escape them. Never apply raw/safe-HTML filters to body text. Richer
article structures remain #26.

AppError separates NotFound, Validation, Conflict, Forbidden, Unavailable and
InvalidData. It retains no driver error/cause, SQL, URL or private payload. Logs
include a static operation and safe SQLSTATE only. Permissions are not confused
with missing records. Pool acquisition is bounded at 3 seconds; statement/lock
timeouts are 5/3 seconds. Cancellation/dropped transactions use SQLx rollback.
Complete HTTP error mapping remains #14.

create_draft inserts story/revision/pointer in one transaction. It does not
authenticate, approve, publish or enforce writing length; those remain
#15/#16/#28/#29. Trusted services must supply the authenticated actor before
exposing mutations. Tests inject a revision rejection to prove no partial story
survives rollback, and check uniqueness conflicts/read-role denial.

## Source identity follow-up

Review found immutable citation rows did not freeze their target URL. Migration 3
rejects source-article ID/source/canonical-key/original-URL changes from creation.
Extraction body/state/metadata can change subject to usage constraints. Corrected
targets need a new source record and a new story revision/citation. Freezing at
creation avoids a race between source identity editing and publication.

Migrations 1/2 remain checksum-compatible. v2 -> v3 is additive; the new binary
requires all three checksums. Older exact-version readiness rejects newer schemas;
use the release compatibility plan, not an assumed automatic rollback. No live
DDL is run here.

The disposable integration wrapper covers typed reads, keyset traversal,
withdrawn snapshots, malformed blocks, restricted roles and transaction rollback.
Ordinary cargo test intentionally skips the opt-in DB suite; CI invokes the
wrapper explicitly.
