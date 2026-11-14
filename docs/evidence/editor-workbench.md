# Editor workbench model evidence (#29 slice 1)

The model slice preserves original historical provenance during text corrections,
blocks contradictory original-work declarations for historical kinds, exposes bounded
private draft pages and reads actual frozen evidence as explicit proposals. It adds
no schema, dependency or HTTP publication endpoint. #29 remains open for its interface
and complete author/review/publish/correct/withdraw journey and curation controls.

Passed locally:

- `cargo test -p world-news --locked --lib`: 51 unit/in-process tests.
- Focused real citation/workbench integration, followed by all 19 disposable DB
  suites and five Chromium regressions with tmpfs/completed-binary cleanup enabled.
- `cargo fmt --all -- --check`, strict all-target Clippy, locked build, runtime
  smoke, foundation/GADS drift, deployment-policy and narrow secret-pattern checks.
- `git diff --check`.

New cases verify keyset pages and state filters, retained origin versions after a
new assertion, publication refusal for stale pinned proof, proposal reads leaving
current citations empty, genuine archive text correction retaining historical kind/
lineage and the old publication, fake-kind rejection, reader authority denial and
live revocation. Negative version and contradictory historical/original declarations
are rejected. Existing source, length, auth, pipeline, structured-body and thread
regressions remain passing; no new UI completion is claimed by those regressions.

A broad attempt hit a linker SIGBUS on this small VFS workspace after disk pressure.
Only obsolete task executables/failed-link temporaries were reclaimed; the final
complete suite passed. Local optimization/strip settings and optional fixture cleanup
are those recorded in article-length evidence. No assertion was skipped or weakened,
host prune performed or running environment changed.

Self-review checked same-story proposal lookup, live-session permissions, exact
reference versions, bounded cursors, no recursive provenance copies, immutable old
publications and explicit historical identity. Source packages keep private notes
out of public projections. This is not independent review, a truth certificate,
performance acceptance or live release evidence. Roll back with a compatible gated
writer/forward fix; do not delete history or revert to a bypassing publisher.
