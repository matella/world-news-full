# ADR 0029: private versioned edition drafts without implicit article publication

Status: accepted for implementation. Related: #30/#31; extends ADR 0028.

## Decision

Persist deliberately selected current publications in the existing edition identity
and membership tables. Editors and publishers may create/save private edition
drafts through a live-session service. This slice exposes no HTTP endpoint or
edition publication operation. Article publication remains its own explicit
reviewed command; a private or withdrawn article cannot be selected by inventing
a readiness flag. Approved private candidates must first be explicitly published
through the article workflow before this initial draft service selects them.

Each save supplies an expected edition version plus exact story revision/version
fences. Lock live identity/session, then edition, then stories in sorted stable-ID
order. Validate all selections before replacement, and commit membership, next
edition version and private before/after audit atomically. A stale/private/withdrawn
selection rejects the whole save. There is no automatic refresh or partial save.
Empty/no-lead drafts may be prepared with warnings; duplicates are rejected before
persistence. Existing published editions remain immutable.

Migration 19 adds edition lock_version (existing rows start at zero), nullable
prepared_story_version on memberships and an immutable private edition_actions
journal. Existing imported identities, order, revision pointers and public views/
grants are unchanged. Historical rows get no invented actor, preparation fence or
audit event. New creation/save actions record the real live actor and next version.

Private reads lock the edition while observing its entries, report current story
changes without rewriting saved fences, and retain unknown historical preparation
versions. Use the existing pure composition projection with actual stored revision
content and revision publication-event instants. Use the database observation clock;
never infer publication age from revision creation. History is bounded to 20 rows
with an exclusive action cursor; preparation is bounded to 100 entries and 8 MiB
of aggregate JSON body text before loading/decoding it. Oversized stored content
fails explicitly, without truncating or losing the prior selection.

## Authority and compatibility

Test fixtures grant a narrow writer draft/membership/audit access and only column
updates needed for row locking/version changes. They do not provision live roles.
Private notes/actors/fences are absent from public views and anonymous users gain
no authority. A draft save does not alter article/edition publication state.
Timezone drift from the edition's stored identity is visible and prevents editing
under a different configured identity; it never rewrites imported dates.

Schema readiness is exact: schema-18 binaries refuse schema 19 even though reader
view shapes are unchanged. Rollback requires a compatible binary/forward fix or a
separately verified restore plan. Never delete edition history, reset versions or
flatten snapshots to accommodate an older image. No live DDL or deployment.

## Remaining work

Protected draft browsing/story choices, nonpersistent selection UI, explicit
publisher edition publication, snapshot/removal policy and full reader notices
remain #30/#31. These are separate gates; do not close either issue on this service
slice. See docs/edition-composition.md and its service evidence.

Import events record import bookkeeping, not original publication instants. Only
explicit `published`/`corrected` revision events supply publication age; an
import-only revision remains unknown even when the import event has a recent
created_at. Do not substitute edition dates or revision creation times.
