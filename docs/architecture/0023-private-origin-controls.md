# ADR 0023: human article-origin controls with sealed history

Status: accepted for implementation. Related: #29; extends ADRs 0017 and 0022.

Expose article-specific origin assertions through the existing private editorial
boundary. A live publisher can prepare source-specific attribution, select registered
origin identities and save a sealed assertion. Editors can inspect the current
version, bounded history and private details. Registry relationships remain hints;
opening a document or selecting a label never certifies its reporting origin.

Read the actual article pointer/version and sealed evidence through an authorised
service transaction; do not infer current state from a history page or registry.
Missing assertions mean unknown only at version zero; inconsistent pointers fail
honestly. Choice pages are bounded and retain selected identities. Oversized invalid
selections show only the selected choices so they can be deselected without growth.

Use native forms under the existing bounded private editor path. Add/page actions
do not write; saving requires the exact current version and a fresh private human
justification. Previous notes remain immutable history rather than prefilled new
attestations. Public basis and private notes are clearly separated. Stale/validation
responses preserve escaped submitted fields after live publisher reauthorisation.
Both HTTP guard and domain transaction require publication permission for writes;
Origin/CSRF and strict bounded form encoding remain mandatory.

Saving calls OriginEvidence::assert, preserving its lock order, version fence and
append-only evidence. Existing citations retain their captured proof versions;
publication still requires explicit citation refresh/review after proof changes.
An assertion does not publish, change intake state or trigger source HTTP. Actor
and private proof remain excluded from public citation views.

No schema, dependency, runtime scheduling or production change. Rollback removes
these controls while retaining all evidence and compatible reader behaviour. Origin
registry administration, thread controls and the complete publishing UI/journey
remain #29 work; these controls add no HTTP article-publication endpoint.
