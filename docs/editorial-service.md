# Editorial service contract

Issue #16; ADR 0004 describes public/candidate state and locking. This service does
not yet expose publishing over HTTP. Public reading continues through the existing
restricted views. Editing UI, citation/word-budget checks and edition publication
are separate required work before deployment.

## Operations and permissions

| Method | Permission | Effect |
| --- | --- | --- |
| create(session,id,input) | Editor or publisher | Atomic story + immutable revision 1 + actor/action; conflict on occupied ID |
| revise(session,id,expected,input) | Editor or publisher | New numbered revision and draft review state; preserves published pointer |
| submit(session,id,expected) | Editor or publisher | Nonempty candidate enters review; currently published revision cannot be resubmitted in place |
| review(session,id,expected,outcome,note) | Publisher | Append exact-revision approval/changes-request and update candidate review state; changes require a private note |
| publish(session,id,expected,public_reason) | Publisher | Require exact candidate approval; atomic first publication or correction event/pointer/action |
| withdraw(session,id,expected,public_reason) | Publisher | Withdraw published body, preserving all revisions and any pending private candidate |
| head/revision/history | Editor or publisher | Authenticated private reads, rechecking the session; history returns <=20 metadata rows with exclusive before_number |

Every existing-story mutation requires the head's current lock version. Refresh
and reconcile on Conflict; never retry by silently fetching and replacing the
version. Two writers using one version yield one success and one conflict. No
force overwrite, delete-revision, restore-withdrawn or automatic publication method
exists. A withdrawn article is terminal under v1; a later restoration feature needs
an explicit reasoned transition and tests, not a state assignment.

RevisionInput has title, optional standfirst/summary, typed paragraph/headings,
language, format/kind, topics/geography and a change reason. It has no actor/role.
Titles are single-line 1–500 characters; optional metadata is nonempty when present;
standfirst/summary limits are 1,000/3,000 characters; at most 20 topics and 20 geography
labels, each 1–100 characters. Reasons are 1–2,000 characters, private review notes
1–10,000. Bodies retain the existing 1 MiB/2,000-block structural cap; NUL is rejected
before Postgres insertion. Draft body can be empty; submitting cannot. No padding,
truncation, automatic geography/language inference, translation or claim attribution.

An approved publisher may review their own work in the initial small-team model.
This is transparent explicit approval, not a claim of independent verification.
Article-length/structure/sourcing publication gates are not yet implemented. The
service's minimal state tests must not be presented as editorial acceptance.

## Transactions and evidence

Creation, each save and every state change writes its audit in the same transaction.
Publication events, old revisions, reviews and editorial action records are immutable.
New revision invalidates the old approval; an older approved ID cannot publish the
new candidate. Correction/withdrawal public reasons are available in public_notices;
reader presentation remains #36. Private review notes never enter that view.

A session passed to the service is revalidated in its transaction. Current user
role/active status and session expiry/revocation are row-locked before story access.
This prevents stale cached identities from authorising a later write. No transaction
is held across a human editing interaction. Concurrent role revocation waits/serialises
against the action; once a revocation commits, a waiting action rechecks and fails.

The editorial runtime role extends the auth runtime grant set with SELECT on
stories, story_revisions, editorial_reviews, editorial_actions, publication_events;
INSERT on those tables; UPDATE(state,current_revision_id,published_revision_id,
review_state,lock_version,updated_at) on stories; USAGE on their identity sequences.
It gets no UPDATE revisions/reviews/events, DELETE, account/role control or DDL.
Reader credentials cannot select any of this private history/audit. Grant changes
for actual environments need the concrete release plan; no automatic grants run.

`tests/editorial.rs` logs in through the real HTTP auth handlers, then exercises the
production service with the resulting sessions and restricted runtime role. It also
checks public HTTP/repository output, injected rollback, concurrent edits, identity
row-lock ordering, bounded history and preserved edition snapshots. Invoke it only
through `scripts/test_database.py`, which creates its own fresh local DB.

No new dependencies are introduced. Migration 5 does not modify legacy public
objects, promote old content to approved or fabricate reviewers. After an authorised
migration, rollback needs a matching binary/forward fix or verified restore; do not
delete newly recorded review/audit data to downgrade.
