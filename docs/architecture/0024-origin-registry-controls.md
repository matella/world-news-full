# ADR 0024: stable origin identities with private configuration history

Status: accepted for implementation. Related: #29; extends ADRs 0005 and 0023.

Provide private registry directory/create/detail/update screens using the existing
Registry service and pipeline renderer. A live publisher can create a stable origin
identity and explicitly enable it for new assertions. Existing ID, name and source
association remain immutable; forged changes are rejected by the service as well as
read-only form controls. Availability and private configuration notes are editable.
No identity, evidence or history is deleted.

Registry configuration is separate from article-specific proof. Creating/enabling
an identity does not assert any document origin, trigger HTTP or refresh citations.
Retirement prevents new assertions selecting that identity and marks affected
captured evidence for reexamination; historical snapshots remain stored.

Expose a bounded private audit projection with version, actor, time, resulting
availability and private note. Existing save_origin audit snapshots wrap a typed
Record, while older disable snapshots contain raw table rows. Read both shapes
without silently hiding old events or exposing raw JSON to the operator. Audit
rows remain immutable and receive no public serialization/reader grant.

Use the existing bounded editor request namespace for valid Unicode notes. Strict
UTF-8, duplicate/unknown-key rejection, CSRF/Origin, live publisher permission and
version fences apply. Validation/conflict responses retain submitted fields after
reauthorisation. Pipeline textarea rendering uses a sentinel newline to preserve
leading authored newlines; long origin titles wrap in the private interface. A
prepared proof version is labelled as the version used by that input, including
stale recovery, rather than falsely presenting it as the latest stored version.

No schema/dependency or production operation. Test-only curator grants demonstrate
the existing registry authority plus private audit reads, without provisioning live
roles. Rollback removes the screens while retaining compatible registry/evidence
records. Audited thread controls and complete publication acceptance remain #29;
this slice adds no article-publication HTTP endpoint.
