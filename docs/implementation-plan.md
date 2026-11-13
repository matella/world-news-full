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
- Rust foundation branch based on #51: #10, typed config/policy, bounded pool,
  escaped preparation template, health endpoints and real-process smoke test.
- No PR is merged, no issue is closed prematurely, and no live deployment occurred.

Next batch in progress: #11 owned schema and schema-version readiness, followed by
#12 offline/idempotent legacy import and #13 typed persistence. Schema integration
uses an explicitly disposable database; no unknown legacy schema is modified.

PR #53 implements #11 and passed GitHub CI. #13 is implemented before #12 because
both depend on #11 and the importer can reuse typed content/error boundaries.
This changes implementation order, not the dependency graph or product scope.
