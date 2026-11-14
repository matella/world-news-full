# ADR 0020: preserve history while preparing editor candidates

Status: accepted for implementation. Related: #29; extends ADRs 0018 and 0019.

The editor needs private, bounded draft navigation and reference proposals without
turning a proposal into approved evidence. Add live-session read models to the
editorial service; do not give readers access to private tables or expose publishing
HTTP before the complete editor acceptance journey is implemented.

Draft lists have explicit state filters and keyset pagination by recorded update
time and ID. Unknown times remain unknown. Evidence views hydrate bindings from
actual immutable citations, including their captured origin versions. If a prose
edit has no package, the preceding revision's evidence is a marked proposal only;
opening that proposal does not attach citations or restore approval.

An optional binding origin version preserves selected sealed evidence. Omission
continues to capture the current version. This lets the editor retain references
until an explicit refresh; stale retained evidence still fails publication. No
caller-provided snapshot or version can bypass the existing database capture/fence.

Text revisions retain original provenance and an immutable parent revision ID, while
replacing sourcing and length assessments. Do not embed whole previous provenance
trees recursively. Changed classification fields must not retain an unrelated old
classification attestation. Genuine imported historical prose may be corrected only
while retaining its historical kind and import lineage; neither a new draft nor an
ordinary story can manufacture that identity. A correction remains a new candidate,
and the old publication stays until review and all publication gates pass.

No schema/dependency, origin inference, source fetch, public route or production
operation is introduced. Full editing/review/correction/withdrawal screens, source
selection, publisher origin/thread controls and complete browser journeys remain
#29. This read/model slice does not complete that issue.
