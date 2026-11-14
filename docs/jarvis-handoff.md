# Jarvis independence and staged writer handoff (#23)

This is a preparation runbook. It does not authorize contact with a live writer,
account/grant changes, final import, stopping Jarvis, merging main or switching
traffic. Main currently has deployment workflows: source review and live release
are distinct steps. Record the affected environment and exact release effects
before requesting the final environment-specific authorization.

## What the source change does

The transitional Next reader no longer generates external analysis links or reads
NEXT_PUBLIC_JARVIS_URL. The private-LAN fallback and ask-URL helper are removed.
Its protected admin page reads at most 200 recent publications from its own archive
and no longer reads news_status or treats historical publisher estimates as local
queue health. The Rust /admin/pipeline interface reports owned local observations.
Neither web application starts or stops any external writer.

Archive text, IDs, dates, translations, counts, source links and original metadata
are preserved. Reader copy labels recorded references and historical publication;
it does not claim a new team-authored or independently verified article. The Rust
legacy import retains pipeline=legacy_jarvis, review=unverified and the original
record hash/timestamp semantics. Historical provenance is not a runtime dependency.
The UI removes promised future synthesis/analysis rather than inventing a new
completion time. No external publication or provider is called by this change.

## Gate 1: inventory and staging rehearsal

Record current source revision/image, actual database/schema and role ownership,
archive export format/counts/date bounds, writer deployment/control mechanism,
ongoing schedules and every process that writes the archive. These production
facts are not verified by repository inspection. Do not infer them from an old
status snapshot or try private-LAN addresses from removed configuration defaults.

Prepare a disposable restored copy using #47's reviewed backup/restore procedure.
Keep operator/migration, editor, worker and public reader identities distinct.
Apply versioned migrations explicitly, import a reviewed offline export first in
--dry-run mode and then with --apply on that disposable database. Reconcile row
hashes, IDs/URLs, edition dates, quarantine reasons, translations, origin metadata
and repeat-run results. An attention-required import is not successful parity.
Do not infer missing reporting origins or rewrite legacy authorship.

Verify a manual original article -> exact revision review -> edition selection ->
publication -> correction journey with no Jarvis/AI configuration or network access.
Check public URLs, citations, word-budget exceptions, edition snapshots and reader
access against the expected revision. This full journey remains a gate on #26–#31
and #48; the current service tests/reader-link removal do not satisfy it. Confirm
backup restoration and migration/rollback rehearsal, image/config reproducibility,
least-privilege roles, health/freshness and acceptable performance under #44–#48.

## Gate 2: concrete release record and authorization

Prepare the exact commit/image digests, migration/checksum list, reviewed runtime
identities and grants, final export/import commands and validation report, reader
routing changes, source allowlist/scheduling policy, writer-freeze mechanism,
maintenance window, stop/resume commands, rollback checkpoints and accountable
operator. Keep credentials outside the repository and release report.

Obtain authorization for that specific environment and those specific effects
before executing the freeze, grants, data import or traffic changes. The general
migration request does not identify a live process or grant an unreviewed stop.
Do not make this authorization request until the concrete artifacts and staging
rehearsal are ready. No such live step is performed by this PR.

## Gate 3: authorized final handoff

1. Capture a recoverable backup and baseline reconciliation report. Keep the
   original archive and provenance; no destructive reset or drop is required.
2. Freeze only the identified legacy writer/schedules using the authorized
   mechanism. Confirm its last committed watermark and no in-flight transaction
   or other writer. Record the evidence; missing status data is not proof of stop.
3. Take the final export after the freeze. Dry-run/reconcile it, then explicitly
   apply it with the authorized migration/import identity. Resolve quarantine and
   compare counts/hashes/URLs and the last-commit watermark before proceeding.
4. Establish the reviewed single-writer ownership. Keep the independent Rust
   publisher disabled until legacy writes are demonstrably stopped; do not allow
   both systems to write a shared publication model. Do not guess live grants.
5. Start the reviewed Rust web/editor path and only the explicitly selected worker
   commands/schedules. Manual publication must work with ingestion and AI disabled.
   Verify the first controlled publication/correction, public revision/edition,
   reader access and health/freshness before changing reader traffic.
6. Switch traffic only under the authorized release plan, retaining old images,
   configuration and backup evidence. Monitor actual failures and content freshness;
   queue activity or a process 200 alone is not publication success.

## Rollback checkpoints

Before Rust publishes new content, stop only the newly authorized writer, verify
its inactivity and use the rehearsed reader/schema-compatible rollback. Resume the
identified old writer only after proving there is exactly one writer and that its
archive/schema/watermark is still compatible. Do not delete imported history.

After Rust has published or corrected content, an old database/image rollback can
lose or hide new work. Preserve the current database and an export of new revisions,
editions, corrections, attribution and audit before any change. Use a rehearsed,
reviewed reverse reconciliation if available. The reverse migration/export is not
implemented or certified by this source slice. If compatibility is unproven, hold
writes and serve the last verified read path while the operator resolves it; never
silently restore an old snapshot over new editorial data or run both writers.

Record timestamps, versions, reconciliation results and every authorized action.
Do not declare the writer retired until the release journey and rollback gates
have evidence. Removing a hyperlink is not operational retirement.
