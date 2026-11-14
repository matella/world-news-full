# Editing article sources

From an existing private article form, choose “Attribuer les sources et les
affirmations” (`/admin/editor/story/{id}/sources`). The screen requires the existing
live editor/publisher session. It uses the configured editorial service and writer
pool; public readers receive no access to private source packages.

Choose a truthful sourcing mode, public attribution and public limits. Select
registered document records and associate each with a real article claim or a
general reference; record a precise location where useful. Browse document/claim
pages without saving. Current choices remain visible across pages. Document intake
metadata remains available in a separate private tab; no source HTTP is performed.

For original work, declare the actual observation, interview or document review,
its public attribution, covered claims and private justification. Select a claim,
then keep it without saving to add another. Unchecking a covered claim removes it.
An additional selected claim is also included on save. Private justifications do
not become public source-package fields. The service rejects missing/invalid scope
or declarations; this form never manufactures a method or original-reporting proof.

A disagreement needs at least two distinct covered claims with attributed positions.
Add positions as needed; deletions of references, methods, disagreements or positions
require explicit checkboxes. Invalid mappings return the submitted form unchanged
for correction. Historical kinds still require historical sourcing mode.

References opened from a preceding text revision are labelled proposals and remain
unattached until an explicit save. Existing references retain sealed origin versions.
Check refresh to capture current evidence; changing a document requires that explicit
capture instead of reusing the previous document's proof number. Where one document
has multiple bindings, keep its selected version consistent across them. Stale proof
can be retained in a draft but blocks publication until refreshed and reviewed.

Save requires a private revision reason, preserves prose and creates a fresh private
candidate requiring review. It does not publish. A stale form returns 409 with its
input for manual comparison; it never overwrites newer work. No durable local
recovery/autosave is provided. Extremely large packages exceeding the bounded form
cannot be edited through this screen; stored packages/history remain intact.

See ADR 0022. Publisher origin/thread curation, complete prepublication checks and
the review/publish/correct/withdraw journey remain #29 gates before HTTP publishing.
