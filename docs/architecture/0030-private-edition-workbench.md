# ADR 0030: bounded private edition navigation and nonpersistent preparation

Status: accepted for implementation. Related: #30; extends ADR 0029.

## Decision

Add private live-session read models before the native edition composition screen:
20-card date/identity keyset pages, explicit draft/all filters, current-publication
story choices and nonpersistent submitted selection previews. These methods use
the editor pool and existing auth service; no public DTO or new runtime grant.

Choices read actual current published revision metadata, format/nature, language
and deterministic taxonomy section proposals. Page 20 eligible publications by
stable ID and retain at most 100 explicitly selected identities, including a now
withdrawn/private selection. Label current state and missing publication metadata
explicitly; never substitute a private candidate title or make a missing revision
successful empty public content. Retained observations do not refresh saved or
submitted revision/version fences and never certify publication readiness.

Nonpersistent preparation validates requested story/revision ownership, then uses
the bounded existing revision projection and database clock. Keep duplicate own
input visible with warnings, distinguish private/withdrawn/stale selections and
preserve explicit order/lead. No membership, version or edition audit event is
written. Final save still reauthorises, locks and rechecks all exact publication/
version constraints; a preparation response cannot grant write authority.

Cards expose only bounded private metadata/counts and an exclusive date/ID cursor.
Choices expose current publication metadata, distinct from a frozen prepared
revision. Large article bodies are not loaded for choices; full preparation keeps
the existing 8 MiB pre-decode bound. Import-event clocks remain bookkeeping and
never become original publication instants.

## Consequences and verification

Use the restricted writer integration to cross page boundaries, retain withdrawn/
private choices, preserve unknown publication fields, reject forged revision/story
pairs, show duplicate/stale own input without persistence and recheck revoked
sessions on every read. Inspect the complete draft/citation/reader regressions.
The native UI and full browser composition journey are still #30 work; explicit
transactional edition publication/removal policy remain #31. No HTTP endpoint,
schema/dependency, source/provider network or live operation. Rollback removes
these consumers and preserves schema-19 histories and frozen memberships.
