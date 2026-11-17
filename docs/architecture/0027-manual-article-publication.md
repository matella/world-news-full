# ADR 0027: manual article publication with public lifecycle notices

Status: accepted for implementation, subject to complete #29 verification.
Related: #29; extends ADRs 0004 and 0020–0026.

## Decision

Expose the existing editorial service through authenticated native forms for
submission, review, publication/correction and withdrawal. The workflow page shows
the current private candidate separately from its published revision, citation and
length blockers, classification warnings, and bounded private revision/review/event
history. Historical previews are read-only. Observed readiness never grants write
authority: the service rechecks live roles, expected versions, approval, current
citation evidence and configured length policy transactionally.

Submission requires an editor; review/publication/withdrawal require a publisher.
Strict forms reject unknown/duplicate fields and accept an expected story version
and revision, never an actor or ready flag. Existing CSRF/Origin checks apply.
Validation/concurrency recovery retains submitted input and old fences after live
reauthorisation; it does not silently overwrite newer work. There is no persistent
autosave, automatic publication or requirement for AI/Jarvis.

A correction creates and reviews a new private candidate. The previous published
revision remains public until explicit publication. Withdrawal applies to the
published pointer even if a newer private candidate exists; it cannot resurrect
content. Immutable revisions, reviews and events remain available privately.

## Public notices and compatibility

Migration 18 replaces the existing security-barrier public_notices view with the
same columns and retained grants, adding initial publication reasons alongside
correction and withdrawal reasons. No private actors/notes or base-table grants are
added; no stored records are rewritten. Public history uses bounded keyset pages.
Article pages show escaped reasons and configured display-timezone timestamps.
Withdrawn current and explicit historical article URLs return 410 with the public
notice history and no article body or source/length package. Edition snapshots
retain their selected revisions and existing withdrawn placeholders.

This is a new schema contract: readiness compares the complete migration list and
checksums. A schema-17 binary refuses schema 18. A release must migrate explicitly
with authorised owner credentials, then run a compatible reader/writer image. An
application rollback must remain schema-18 compatible; do not delete lifecycle
history or pretend an old image can start. Live migration/deployment remains held.

## Validation and limits

Use disposable restricted reader/writer HTTP tests and a real browser with separate
author, reviewer and anonymous sessions. Include stale candidates, malformed/forged
forms, role changes, source/length gates, public/private boundaries, correction
snapshot preservation and withdrawal. See docs/evidence/editorial-workflow.md.
Edition composition, translations, search and full reader design retain their own
roadmap gates. No independent review or live-release readiness is implied.
