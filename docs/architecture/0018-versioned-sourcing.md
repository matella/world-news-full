# ADR 0018: review sourcing as part of an immutable article revision

Status: accepted for implementation. Related: #27; extends ADR 0017.

Store a bounded source package in private revision provenance. A package contains
explicit body-claim/source-record/location bindings, public attribution/limitations,
private human evidence for declared original methods, and attributed disagreement
positions pointing to actual sourced body claims. No generated claim mapping or
truth score is introduced. A draft may be incomplete; publication may not bypass
the sourcing checks by approving an incomplete draft.

Initial creation/text revision can supply a package. A text edit without one does
not inherit old citation authority. Source-only edits clone prose/language/kind and
original provenance into a new review candidate, replace the package/citations and
invalidate approval. Classification-only edits preserve the exact frozen evidence.
The current published revision remains until a new candidate passes publication.

Use the same bounded report for public reading, private draft warnings and the
publication gate. Counts distinguish conservative canonical-URL documents, declared
outlet registry records and documented reporting-origin IDs. Repeated records of
one document cannot inflate documents; inconsistent origin sets on that document
remain contested. Unknown/contested documents do not certify additional origins.

Publication takes share locks on cited articles and origin identities before the
story head, checks that the candidate/version did not change, and verifies source
currentness and policy within that transaction. An assertion or origin disable
cannot slip between the check and commit. The fixed-path helper is revoked from
PUBLIC. A deterministic blocked-publication test proves the recheck after a change.

The public view appends only an explicit nested allowlist of provenance fields.
The projection function is pure, has no table/privileged access and is safe for
readers to call on their own JSON; the lock helper has separate restricted authority.
Never export the private package or strip a few private keys as a substitute.

Single-source reporting is allowed with clear public limitations. Original work
requires a human method/attribution/private note; it does not manufacture an external
source or an independent-reporting count. Historical metadata corrections retain
actual imported lineage/kind and explicit unverified limitations. Existing imported
publications stay readable without retroactive attestation. Opinion labels must
match the article kind. Humans identify material facts and evaluate the evidence;
software verifies attribution structure, not the truth of the prose.

Migration 16 adds an allowlisted public projection and a restricted lock helper.
After this policy is used, do not roll back to a writer that lacks the gate. Use
compatible reader/writer images and forward fixes; preserve published packages,
citations and private notes. Live migrations/grants remain #47–#49 actions.
