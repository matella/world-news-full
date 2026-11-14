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
- PR #56, based on #55: #14 compatible public URLs, minimal escaped
  published-data views, safe request metadata, input/body/time limits and explicit
  missing/outage/unimplemented states. Restricted-reader DB and real-process checks
  and GitHub CI passed. Full reader design/parity remains in #34–#40.
- PR #57, based on #56: #15 explicit identity bootstrap, role checks,
  private expiring sessions, CSRF/logout, audit and separated DB credentials. Four
  fresh DB suites and Chromium form checks passed. Full editorial permission paths
  are now checked by #16's staged service; no real accounts were created. GitHub CI passed.
- PR #58, based on #57: #16 immutable revision/action history,
  exact-revision review, atomic publication/correction/withdrawal and live-session
  revalidation. All five DB suites and 23 unit tests passed, including concurrent
  edits, role-change locking and injected rollback. No publishing HTTP endpoint is
  exposed before the complete content/editor policy gates are implemented. GitHub CI passed.
- PR #59, based on #58: #17 protected versioned source/feed/origin
  configuration, explicit rights/storage/display policy, immutable private audit,
  soft disable and captured historical attribution. All six DB suites passed;
  no source is fetched or subscribed by registry operations. GitHub CI passed.
- Safe feed branch, based on #59: #18 bounded RSS/Atom HTTP/parser services,
  destination/DNS pinning, due/retry status and durable metadata batches committed
  before cache advancement. 29 unit tests and seven DB suites passed. No startup
  polling loop, arbitrary production subscription or publication side effect.

#13 preceded #12 because both depend on #11 and the importer reuses typed content
and error boundaries. This changes implementation order, not the dependency graph.
Next: #19 conservative article normalisation/deduplication from durable metadata,
then rights-aware extraction/jobs and article/editor/edition policy. Service success does not complete
those issues or authorise a live release.
