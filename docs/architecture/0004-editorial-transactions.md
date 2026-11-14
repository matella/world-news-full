# ADR 0004: review a candidate without hiding published content

Status: selected for #16; source services only, no publishing HTTP endpoint or
production publication. Depends on local identity PR #57 and owned schema.

A published article can have a newer private candidate. One overloaded story
status would hide the approved public body while that candidate is reviewed.
Keep stories.state/published_revision_id as public state, current_revision_id as
the working candidate and a separate review_state for that candidate. Saving a
revision resets review; the published pointer changes only on an explicit approved
publish transaction. Editions retain their selected immutable revisions.

Review states are draft, in_review, approved and changes_requested. Unpublished
stories move draft -> in_review -> published; requests for changes return them to
draft. An already published story stays publicly published throughout these steps.
Publication of a later approved revision emits a correction with a public reason.
Withdrawal targets the published revision even if a newer draft exists, removes
all public bodies through existing views and retains history. Withdrawal is
terminal in this initial service: there is no silent restoration/republish path.

Every mutation takes an expected lock_version, locks the story and rejects stale
work. Immutable revision numbers and editorial_actions provide actor, timestamp,
reason and one committed action per new lock version. Review outcome/internal
note stays in immutable editorial_reviews; public reasons go to publication_events.
No private note is copied to a public event. New legacy classifications cannot be
created through normal editing; legacy records require their explicit importer.

The service accepts a server-authenticated Session, never an input actor or role.
Each transaction locks the current active identity first, then its unrevoked,
unexpired session, then the story. The identity/session share locks stay until
commit, ordering operations against account revocation/role changes. Operator
revocation uses that identity-first order. Permissions are re-read inside the
transaction; a previously authenticated cached publisher role is insufficient.
On denial, roll back and append a separate private denial event with the known
subject, without attributing a revoked bearer as a fresh authenticated actor.

Editors can create/save/submit/read private work. Publishers can additionally
approve/request changes, publish/correct and withdraw. Initial policy permits one
publisher to review their own submission; no independent second human is claimed
or required. Review is explicit and tied to the exact revision. Saving even after
approval resets it; publish also checks the latest recorded review for that ID.

Only head/single revision and 20-item metadata history reads are exposed by the
Rust service, with an exclusive revision-number cursor. No full-body unbounded
history query, automatic publish, provider call, HTTP API or editor UI is added.
Future controllers call this same boundary and retain #15 Origin/CSRF protection.

Minimal validation rejects malformed metadata, unsupported kinds, NULs and empty
review bodies. Citation verification, structural quality and configurable word
budgets/overrides remain #26–#28. No production publishing endpoint may expose this
incomplete policy path; #29/#48 must prove the complete manual journey first.
Creating a story does not invent or publish an edition; composition is #30/#31.

Migration 5 is additive: existing records gain no invented review/identity. Legacy
import remains a distinct historical-publication operator path. Runtime DB grants
permit the required source/service operations but cannot change identities/roles,
mutate revisions/events, delete history or own schema. These grants are tested on
disposable roles, not applied to a real environment.

Rollback follows exact-version readiness: before apply revert source; after apply
retain a matching binary/forward fix or a separately verified restore plan. Do not
auto-drop the review field or audit records to make an older binary start.
