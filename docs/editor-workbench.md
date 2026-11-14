# Editor workbench model (#29, first slice)

The private service now provides `drafts` with candidate/in-review/published/
withdrawn/all filters. Pages are bounded by `PageSize`; the exclusive cursor contains
the last recorded update time and ID. Ties use ID order and unknown dates sort last.
Missing/corrupt revision data is an error, not an empty successful board. No HTTP
board or publication operation is added by this model slice.

`evidence_draft` returns an authenticated head, revision and private source package.
Its bindings come from actual citations, with captured origin versions. If a text
revision has no package, `edited_from` can supply a same-story preceding package
as a **proposal**. `proposed` stays explicit; the current revision still has no
citations until an editor commits a valid package. Old missing anchors/duplicate
references require human resolution, never automatic guessed mappings. Private
notes remain private and the result deliberately has no Debug/Serialize adapter.

Bindings accept optional `origin_version`. Absent/null means capture current evidence,
as before; a selected nonnegative version preserves that sealed snapshot. A changed
origin still blocks publication until explicitly refreshed and reviewed. The database
continues to reject future/unsealed versions, mixed versions for one source record
and forged snapshots. Optional omitted fields keep old package JSON compatible.

Body revisions now preserve original provenance and `edited_from` without copying
old sourcing/approval or an old length exception. New explicit input supplies new
assessments. Unchanged classification retains its attestation; changed tags drop that
old attestation. Historical text edits require genuine `legacy_jarvis` lineage and
the same historical kind. Historical kinds require historical attribution rather
than a contradictory new-original-work declaration. Creating a fake historical draft or relabelling an imported
archive as new reporting is rejected. Old revisions/public snapshots remain intact.

Verification extends the real citation integration suite with authenticated list
pagination/filtering, frozen-version retention, unattached suggestions, genuine
archive correction, fake-kind rejection and reader/revoked-session denial. The
complete #29 interface and author/review/publish/correct/withdraw browser journey
remain to implement; no live account, data, grant, writer or deployment change.
