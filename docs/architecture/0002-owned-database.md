# ADR 0002: app-owned schema and immutable publication history

Status: implemented for #11; no production migration performed.

## Ownership and storage

Reserve `news` in a new/staging database. Never infer or alter the legacy public
tables. Embedded SQLx migrations are explicit operator commands, checksum-verified
and tracked in `news._sqlx_migrations`; web startup performs no DDL. Conflicting
objects fail rather than being replaced. Verify the target namespace before use.

Use bounded opaque text IDs to preserve legacy URLs; new records default to native
Postgres gen_random_uuid()::text (no extension on supported Postgres 18). All instants
are timestamptz. Edition dates carry an explicit publication timezone validated
against IANA by the application, rather than adopting the database host timezone.

| Relations | Invariants |
| --- | --- |
| sources, reporting_origins, feeds | Outlet, reporting origin and polling configuration stay distinct |
| source_articles, source_article_origins | Canonical-key uniqueness; metadata-only records cannot retain body text |
| stories, story_revisions | Separate current/draft and published pointers; same-story ownership FKs; append-only revisions |
| citations | Claim-to-source references; immutable once published, locked against concurrent publication |
| translations | Immutable language variants bound to an original revision and explicit review/provenance state |
| editorial_reviews, publication_events | Internal notes stay private; append-only public actions retain actor and reason |
| threads, thread_stories | Stable many-to-many subject identity; legacy hints do not imply automatic merges |
| editions, edition_entries | Day/zone uniqueness, ordered membership, at most one lead, immutable published snapshots |
| jobs | Durable identities, attempts/leases and idempotency keys for the future worker |
| legacy_records, legacy_links | Original payload/hash-to-revision mapping and unresolved relationship hints |

JSONB stores body blocks, provider/legacy provenance and source metadata. Relations,
citations, pointers, ordering and scheduling use typed columns/constraints/indexes.
JSONB type checks do not replace typed block/URL validation in #13/#26.

Publishing/correcting requires an audit event before changing the public pointer
in the same transaction. Withdrawal requires a corresponding event. New drafts
can advance independently of the published pointer. These consistency rules are
not an authentication/review workflow; #15/#16/#29 implement those service rules.

## Read boundary

Grant readers SELECT on public_* and schema_version views plus schema USAGE only.
Security-barrier views exclude actors, raw provenance and internal review notes.
Visible revisions are current published revisions or those held by published
editions. Withdrawals preserve membership/notices while hiding body content.
Draft translations stay private. A trusted migration owner creates views; reader
roles must not have CREATE, base-table SELECT or mutation privileges.

Migration/editor/worker/reader roles are provisioned separately. Migrations do not
create live passwords/users or silently change outside database permissions.

## Upgrade and recovery

1. Discover/back up the authorised target and confirm news is reserved/empty.
2. Run world-news --migrate with the migration-owner connection.
3. Provision runtime grants and validate in staging before switching any traffic.
4. Readiness requires every embedded version/checksum and no unsupported extra
   version. Empty, partial, drifted or unknown schemas return 503.
5. Import an authorised export separately (#12). No implicit live copying or
   external publisher stop occurs during migration.

Migration 1 adds core storage; migration 2 adds editions/public views. Each is
transactional; namespace/metadata bootstrap can remain after failure. Test the
initial empty -> v1 -> v2 upgrade and idempotent rerun. Never edit applied SQL.
Later upgrades should be additive while older app versions coexist.

No automatic down migration or DROP is shipped. Revert app code while retaining
the isolated schema; recover data by restoring a verified backup into a separate
database. Immutable history deliberately blocks ordinary destructive edits;
lawful erasure needs an explicit operational procedure. Search indexes and worker
execution arrive with their actual requirements. No editorial truth claim follows
from a valid database constraint.
