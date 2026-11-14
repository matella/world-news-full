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
| assign | Add a story or replace its explicit UTC update instant; reject another primary assignment |
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
