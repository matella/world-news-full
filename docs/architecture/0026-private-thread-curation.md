# ADR 0026: explicit private thread curation and exact-state inverses

Status: accepted for implementation. Related: #29; extends ADR 0025.

Wire the existing Threads service into protected native forms using the private
pipeline renderer. Editors may inspect cards, members and audit actions; every
unsafe request and domain mutation requires a live publisher. Curation changes
subject identities/relationships, not article prose or publication pointers. All
writes go through the service, never direct graph SQL or similarity/legacy inference.

Expose creation, canonical title/state edits, explicit member assignment/date
correction, removal, subset split, merge preparation/confirmation and inverse review.
Story choices are paged with retained input; a stable-ID lookup prepares a selected
story without assigning it. Known instants require an explicit offset; unknown must
be chosen explicitly with the date empty. Never infer an instant from request time,
publication, feed metadata or bookkeeping. Reasons are private one-line audit text.

Merge preparation reads both canonical active identities and checks the combined
membership bound without mutation. A separate confirmation shows both memberships,
captured versions and destination; it has no editable destination selector. Final
merge rechecks both versions and all domain constraints. Private merged identities
remain inspectable with a destination link and immutable history.

Inverse review displays owned recorded before/after states and current story-title
metadata. Only an observed exact match offers an inverse form, and final undo still
relocks/rechecks all affected states/versions. Caller snapshots and ready flags are
not accepted. Changed/already-undone actions preserve the submitted reason for
recovery and require a new explicit decision. No identity/history reset or deletion.

Strict bounded UTF-8/field parsing, CSRF/Origin and live role checks apply. Native
choice buttons retain unsaved fields without writes. Validation/conflict responses
preserve expected versions and submitted fields after reauthorisation; they never
refresh fences automatically to overwrite newer work. No durable autosave or silent
merge is introduced. Titles displayed for historical member identities come from
current revisions and are labelled accordingly, not fabricated historical prose.

No schema/dependency, source HTTP, worker scheduling, article-publication endpoint
or production change. Rollback removes these consumers while preserving all graph/
audit/evidence records. Complete prepublication/review/publish/correct/withdraw UI
and browser acceptance remain #29 work before exposing article publishing HTTP.
