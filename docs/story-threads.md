# Stable story threads (#25 core)

Threads describe persistent subjects. Stories represent dated developments, and a
story has at most one primary subject. Separate topical/geographic tags continue
to support multiple values. Membership/merges are editorial decisions, not output
from a similarity score or a legacy related-ID cluster.

Migration 14 extends the owned schema. It refuses old stories assigned to multiple
threads and threads with more than 200 members: reconcile those actual records
under a reviewed upgrade plan rather than accepting a silent choice. Old membership
update times remain NULL. The bookkeeping added_at timestamp is retained, but is
not claimed as a development/publication time. No legacy graph is automatically
assigned or backfilled into subjects.

## Protected service API

Construct Threads with the editor runtime pool for private operations. Each call
reauthorizes the live session; editor role may inspect private state/history, while
publisher role is required for all curation that could affect public relationships.
Reasons, titles, IDs, expected versions and input sizes are validated. The runtime
uses short one-second lock/five-second statement budgets under a graph advisory
lock. Member replacement is batched per thread. These are per-statement/lock
budgets, not a hard total wall-clock deadline. No source HTTP, article publication
or worker execution occurs.

| Method | Behavior |
| --- | --- |
| create | Stable caller-supplied ID, title, initial active state and immutable action |
| edit | Versioned title/active/closed change on a canonical subject |
| assign | Add a story or replace its explicit known UTC instant / explicitly unknown instant; reject another primary assignment |
| remove | Remove an explicit member, preserving it in audit history |
| merge | Move source members to target; close source and retain its ID as a bounded alias |
| split | Move a nonempty explicit subset into a new ID, leaving members behind |
| undo | Append an inverse action when all affected states/versions still match |
| get/history | Private current snapshot and bounded keyset audit metadata |

No identity or audit record is deleted by undo. A newly created thread becomes an
empty closed identity when its creation/split is undone. Undoing the inverse can
redo the earlier decision. If another change touched either subject, stale undo
conflicts; inspect current state and make a new explicit decision. Never restore
old JSON directly over newer memberships or reset versions to force an inverse.

A merged source is not an editable canonical subject; use the destination or the
matching inverse action. The maximum alias chain is twenty edges. A merge that
would exceed it is rejected. Closed canonical subjects with public members retain
a latest public head/history; empty closed identities remain publicly unavailable.

The editor-runtime database role needs reviewed SELECT/INSERT/UPDATE on threads,
SELECT/INSERT/DELETE on thread_stories, SELECT/INSERT on thread_actions, SELECT on
stories, and the audit sequence alongside existing auth-runtime privileges. It
needs no DDL, article-body writes or public-resolver execution for these operations.
Disposable tests use those grants, not owner credentials. Provisioning live grants
is a separate reviewed release action; migration itself grants none.

## Public queries

Construct Threads with only the public reader pool. Grant SELECT on
public_thread_members/public_thread_heads and EXECUTE on resolve_public_thread(text)
to the reviewed reader identity. The resolver has a fixed search path, no dynamic
SQL, a cycle/depth guard and no PUBLIC execution grant. Private thread tables and
before/after snapshots remain inaccessible to the reader.

- heads returns one latest public head per canonical thread. Default ordering is
  explicit update instant descending, unknown instants last, then stable ID.
- timeline resolves an old merged ID and reads public members in one repeatable,
  read-only snapshot. Concurrent curation cannot make an alias resolve against one
  graph and then return an empty page from another.
- resolve returns a canonical ID only if it currently has a public head. Missing,
  private-only and withdrawn-only subjects are not publicly discoverable.
- PageSize bounds results to 1–100 and an extra row determines the next cursor.
  Cursor time can be NULL; the ID disambiguates same-time and unknown-time rows.
- Filters use canonical topic/geography keys plus the shared policy's explicit
  legacy aliases, an exact language tag or active/closed state. All content filters
  must match the same public member. The returned head is still the latest public
  development, even if an earlier update matched the filter.

Counts distinguish currently public member stories, distinct published editions
containing them, and distinct edition dates. Multiple same-day updates do not
invent additional dates, and a standalone publication can have zero edition dates.
Private/withdrawn members do not inflate public counts. Update instants and edition
publication dates are different data; do not substitute one for the other.

Pages are consistent within a query/transaction, not frozen across unrelated reader
requests. Curation/new publication can move heads between pages. Archive-scale
query plans, graph contention and throughput remain performance work #45.

## Integration and release gates

Core APIs/read models are staged here. Wire editor curation controls under #29 and
stable reader timelines/topics under #39; search consumes them under #38. The old
Next related-ID UI is transitional and is not silently pointed at a different
schema/credential. Do not claim that serving the unmodified old topics page uses
these new subjects. Preserve old hints and import payloads until final parity.

Apply migration only with the reviewed owner and actual ambiguity report. Verify
restricted reader/editor roles, backup/restore and image/schema readiness before
activation. No live migration, new role/grant, reader traffic change or Jarvis stop
is performed by this feature. Rollback stops serving its consumers and retains
identity/audit/membership data; do not delete subjects to revert a release.

## Private curation preparation

`cards(session, after, size)` provides bounded ID-keyset pages containing actual
private thread state/version, alias destination and all-member counts. These counts
must not be presented as anonymous public counts. `story_choices` provides 20 current
revision metadata rows plus retained selections (bounded to 800 for two before/after
snapshot pairs). An absent current revision remains absent metadata and must be
labelled incomplete, never replaced with a made-up title or empty ready article.

`inverse_preview(session, action_id)` reads immutable recorded before/after states
and observes whether all current affected states still match and the action has not
already been undone. It is private preparation only. Final `undo` reauthorises,
locks and checks exact state/versions again; no preview flag grants authority. The
complete operator forms and their browser journeys still belong to #29.

`MemberInput.update_at` is now optional in the Rust service API. `Some(instant)`
means actually known and is normalized only to database microsecond precision;
`None` explicitly means unknown. Known callers must wrap their timestamp in Some.
An HTTP interface must make this known/unknown decision explicit. Never silently
use an empty timestamp, bookkeeping added_at, publication date or current time as
a known development instant. Correcting a known instant to unknown is versioned
and audited; the original added_at stays unchanged. Inverse actions preserve NULL
and monotonic versions rather than inventing recency during undo/redo.

Private story-title choices additionally need reviewed SELECT on story_revisions;
the disposable fixture demonstrates that read grant, without changing any live
identity. No schema/dependency or HTTP curation/publication route is added by this
model slice. See ADR 0025 and its evidence record; reader integration stays #39.

## Protected human curation screens

Open `/admin/editor/threads` from the private pipeline navigation. Editors can
inspect all private/current subjects, including aliases and empty closed identities.
Counts include private articles and must not be interpreted as anonymous reader
counts. Publishers can create a stable identity with a one-line private reason.

`/admin/editor/threads/{id}` presents current members and their explicit development
instant separately from bookkeeping added_at. Canonical identities provide title/
state, assign/date-correction, removal and subset-split forms. Story choices page
without saving; lookup by stable article ID prepares the real selection without
assigning it. A story assigned to another primary subject cannot be silently stolen.
Use an explicit removal, split or merge decision with the appropriate version.
Occupied selections show their current subject; a conflict links to that subject
for inspection rather than moving its relationship implicitly.

A date must contain an offset, for example `2026-11-14T10:00:00+01:00`, or the user
must explicitly choose unknown and leave the date empty. The service stores UTC at
microsecond precision. It never uses publication, fetch, request or added_at time
as a substitute. Correcting/removing a relationship never edits or publishes prose.

Split chooses an explicit nonempty subset and leaves members in the source. Merge
first prepares two distinct active canonical identities, verifies the combined
200-member bound and shows memberships/versions without mutation. Confirmation is
a separate form with the destination fixed. Final merge rechecks both versions.
The source becomes a preserved alias; its private page still exposes history and
the destination link rather than hiding the old identity.

Audit links open `/admin/editor/thread-actions/{action_id}`. Review the recorded
before/after graph and reason. Article labels are current revision metadata, not
historical prose snapshots. A publisher can request an inverse only while observed
states still match; the service checks exact states/versions again under its graph
transaction. A concurrent change rejects the inverse and preserves the submitted
reason. Undoing creation/split leaves a closed empty identity; undoing an inverse
can redo the earlier decision with new versions. Nothing resets versions or history.

All mutations require live publisher permission at HTTP and service boundaries,
CSRF and matching Origin. Strict form encoding and duplicate/unknown-field checks
reject actor/snapshot/ready spoofing. Invalid/stale input is preserved for manual
comparison with current data; expected versions do not refresh themselves. Closing
or navigating away can lose unsaved fields; there is no durable autosave. No source
HTTP or worker job runs. See ADR 0026 and its evidence. Complete editorial article
publication acceptance remains #29; these screens expose no article-publish route.
