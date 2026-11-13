# Implementation record

Roadmap: https://github.com/matella/world-news-full/issues/1

| Batch | Issues | Observable completion |
| --- | --- | --- |
| Foundation | #2, #3, #4, #9 | Baseline/fixtures, pinned GADS audit, ADR and editorial policy |
| Existing app safety | #5, #6, #7, #8 | Durable disposable DB, protected admin, honest errors, reproducible checks |
| Rust baseline | #10–#17 | Runnable web/config, owned schema, import, auth and revision primitives |
| Independent publishing | #18–#33 | Intake/jobs/editor/editions independent of Jarvis, citations/length rules |
| Reader migration | #34–#43 | Required routes and archives with accessible Rust templates |
| Release evidence | #44–#49 | Performance/health/recovery/integration evidence and cutover preparation |

Work on scoped branches/PRs. Completion of a branch is not merge, deployment or
issue closure; link exact evidence and keep remaining gates visible. P0 fixes
can advance once foundational rules are available. Follow issue dependencies;
if a prerequisite is only staged in a preceding branch, stack the PR explicitly.
Never mark 48 issues complete because a scaffold exists. Optional AI remains off.

Current production schema, exposure, storage and publisher are unverified. Local
source implementation and disposable tests continue without production access.

## Current staged work

- PR #50: baseline, GADS, ADR and editorial settings (#2/#3/#4/#9).
  Foundation CI passed. #3 still lacks live Claude Code runtime validation.
- PR #51, based on #50: current-app safety/build/persistence (#5–#8).
  Local and CI verification passed; production/PVC/real-role steps remain unverified.
- PR #52, based on #51: #10, typed config/policy, bounded pool,
  escaped preparation template, health endpoints and real-process smoke test.
- No PR is merged, no issue is closed prematurely, and no live deployment occurred.

- PR #53, based on #52: #11 owned schema, embedded migrations and restricted public
  views. Local disposable-DB verification and GitHub CI passed.
- PR #54, based on #53: #13 typed repositories, bounded pagination and transactional
  draft creation. Local verification and GitHub CI passed.
- PR #55, based on #54: #12 offline plan, explicit atomic apply,
  idempotent ledger and immutable archive snapshots. Unit, CLI and disposable-DB
  verification and GitHub CI passed; real production archive format remains unverified.
- HTTP contract branch, based on #55: #14 compatible public URLs, minimal escaped
  published-data views, safe request metadata, input/body/time limits and explicit
  missing/outage/unimplemented states. Restricted-reader DB and real-process checks
  passed. Full reader design/parity remains in #34–#40.

#13 preceded #12 because both depend on #11 and the importer reuses typed content
and error boundaries. This changes implementation order, not the dependency graph.
Next: #15 authorisation and #16 editorial revision services. Route or import success
does not complete these issues or authorise a live release.
