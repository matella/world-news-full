# ADR 0022: explicit private source-package authoring

Status: accepted for implementation. Related: #29; extends ADRs 0018 and 0021.

Attach sourcing through the existing versioned editorial service rather than
embedding JSON in the article-writing form. The protected source screen edits a
public attribution/limits statement, document-to-claim locations, human methods
with private supporting notes, and attributed disagreement positions. It never
fetches documents, creates evidence declarations automatically or publishes.

Opening same-story parent proposals does not attach references. Saving explicitly
creates a new private candidate with unchanged prose and invalidated review. Exact
head/revision fences reject stale forms and preserve submitted fields for manual
comparison. No caller actor, policy-ready flag or public evidence snapshot is trusted.

Hydrate actual frozen reference versions. Retain them until explicit refresh; a
changed document cannot accidentally inherit another document's numbered proof.
Refresh captures current evidence through the database boundary. Publication still
rechecks evidence currency. Source metadata/registry relationships do not certify
article-specific reporting origins, rights or independent verification.

Provide bounded keyset document pages plus retained choices. Claim dropdowns show
50 choices plus their current selection, avoiding a full body-by-control Cartesian
expansion. Human scopes show selected claims plus an additional claim picker.
Missing proposal anchors remain visible and must be repaired or explicitly removed.
Add/page actions retain unsaved fields without persistence; deletion is explicit.
Extremely large packages exceeding the bounded private form are rejected without
mutating stored data. Domain/package limits and live role/session/CSRF/Origin checks
remain mandatory. Labels are explicitly associated with populated textareas.

No schema/dependency or production change. Rollback removes these screens while
retaining all immutable revisions/evidence. Publisher origin/thread controls and
full review/publication/correction/withdrawal acceptance remain separate #29 work;
this slice adds no HTTP publication authority.
