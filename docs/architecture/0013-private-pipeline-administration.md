# ADR 0013: Private server-rendered pipeline administration

Status: accepted for #22, based on #66. Staged, not deployed.

Use the existing Rust/Axum authenticated administration boundary and Askama forms
for source configuration, pipeline observations, recovery actions and manual
editorial intake. Do not add a second frontend, browser credentials, JavaScript
runtime or worker process to the web server.

Private reads use the configured editor pool and reauthorize the live session in
a transaction. Public reader credentials cannot access these tables. The account
portal retains its narrower authority; the pipeline overview is /admin/pipeline.
Lists use bounded keyset queries and targeted indexes, not unbounded history loads.

Forms call the existing registry, feed, normalization, extraction, intake and
editorial services. They preserve current-session/role/version checks and atomic
history. Editors can read observations, register links, write notes and create
original private drafts. Publishers additionally change source/feed policy,
pause/resume feeds, replay/cancel normalization and grant/cancel extraction.
Authentication, same-origin CSRF and strict form deserialization precede mutations.
Expected stale versions return a conflict rather than overwriting another editor.

A source storage-policy downgrade can remove retained publisher text. Its first
submission produces a concrete confirmation showing the proposed policy; only an
explicit confirmation applies the same expected version. Other operations preserve
provenance and use existing reversible/audited transitions. No UI action runs an
HTTP fetch, starts a daemon, grants worker database privileges or publishes a story.

Keep publisher text rights-gated and separate from human notes. Escape all displayed
source values, use validated original URLs and fixed server-owned action paths.
Read-only drafts demonstrate the private handoff; the full editor/publication UI
belongs to #29 and its policy prerequisites. Timestamps follow the configured
publication timezone. Observed success/age/counts do not imply whole-pipeline health.
