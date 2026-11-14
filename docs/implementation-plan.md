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

## Implementation history (all PRs #50–#71 merged)

- PR #50: baseline, GADS, ADR and editorial settings (#2/#3/#4/#9).
  Foundation CI passed; the available Codex task smoke and unavailable Claude CLI
  are documented in docs/gads-adoption.md. No Claude runtime certification is claimed.
- PR #51, based on #50: current-app safety/build/persistence (#5–#8).
  Local and CI verification passed; production/PVC/real-role steps remain unverified.
- PR #52, based on #51: #10, typed config/policy, bounded pool,
  escaped preparation template, health endpoints and real-process smoke test.
- Integration update: PR #72 retired both push-deployment workflows, then #50–#71
  merged in dependency order under the owner's explicit merge instruction. The
  integration head is 565cf47597a51ac16be3d28bd5c035bdb5a5bb7a. No live deployment
  occurred. Completed issues are closed; partial acceptance remains tracked.

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
- PR #60, based on #59: #18 bounded RSS/Atom HTTP/parser services,
  destination/DNS pinning, due/retry status and durable metadata batches committed
  before cache advancement. 29 unit tests and seven DB suites passed. No startup
  polling loop, arbitrary production subscription or publication side effect. Both
  CI workflows passed.

- PR #61, based on #60: #19 deterministic URL identity, immutable
  entry observations and idempotent receipts; conservative conflicts/changes never
  overwrite existing article identity or text. All eight disposable DB suites and
  30 unit/in-process tests passed. Both CI workflows passed.

- PR #62, based on #61: #20 bounded inert HTML extraction,
  current rights/session checks, private provenance and removable publisher text,
  manual source links/notes and atomic original draft linkage. Nine fresh DB suites,
  33 unit/in-process tests and CLI/process checks passed. Both CI workflows passed.
  No public text reuse, startup worker or protected editor UI is implied.

- PR #63, based on #62: first #21 slice with fenced
  Postgres claims, leases, bounded retries, attempt/operator audit, explicit one-job
  CLI and atomic normalization acknowledgement. Ten disposable DB suites and
  34 unit/in-process tests passed; both CI workflows passed.
  Feed recovery advances in the next slice; machine extraction authority and daemon
  activation remain #21 gates.

- PR #64, based on #63: second #21 slice with leased attempts,
  stale/cache fences, abandoned-work recovery, retry budgets/source minimums,
  publisher controls, atomic machine normalization handoff and explicit CLI.
  Eleven disposable DB suites include process-kill/recovery and upgrade cases;
  34 unit/in-process and final CLI/process/strict checks passed; both CI workflows passed.

- PR #65, based on #64: third #21 slice with expiring
  publisher grants, separate machine snapshot attribution, fenced/rights-aware
  execution, retry/recovery and shared manual/machine source cooldowns. Twelve DB
  suites passed; final local checks/CI are recorded in the evidence and linked PR.

- PR #66, based on #65: fourth #21 slice with an explicit exact
  feed allowlist, opt-in granted extraction, capped stage work and interruption
  reporting. Thirteen disposable DB suites include full intake and real CLI
  timeout/default checks. Final local checks/CI are linked in the evidence/PR.

- PR #67, based on #66: #22 protected measured status,
  bounded source/feed/article/job screens, authorized configuration/recovery/grants,
  manual notes and original private drafts. Fourteen isolated DB suites and real
  Chromium source/feed/intake/draft/mobile checks passed; source storage removal
  requires explicit versioned confirmation. See docs/evidence/pipeline-admin.md.

- PR #68, based on #67: removes console links/configuration
  and the transitional admin's publisher status/ETA dependency while preserving
  archive data and legacy provenance. docs/jarvis-handoff.md prepares the staged
  single-writer cutover; live retirement/full edition journey remain gated.

- PR #69, based on #68: #24 explicit topic/geography policy, deterministic
  sections, unknown legacy proposals and versioned editorial overrides preserving
  content/citations/provenance/public snapshots. 40 Rust tests, fourteen DB suites,
  real admin classification and transitional science-grouping browser checks passed.
  See docs/evidence/taxonomy.md; edition/search consumers remain #30/#38.

- PR #70, based on #69: #25 primary subject identities, explicit
  update instants, deterministic public heads/counts, bounded queries and audited
  merge/split/undo. Fifteen DB suites include concurrent timeline resolution,
  reversal, ambiguity-preserving upgrade and private/withdrawn/alias bounds. Editor
  controls #29, search #38 and reader timelines #39 consume these core services.

- PR #71, based on #70: #26 explicit overview/full-body fields,
  plain claim/context/uncertainty blocks, bounded unique anchors and shared escaped
  public/private rendering. 43 Rust tests, sixteen DB suites and three Chromium
  workflows passed. No inferred legacy sections or rewritten prose. Citation/length
  acceptance remains #27/#28; rich-body rollback readers must support the format.

Next: #28 word budgets after the #27 citation-package gate passes,
article/citation/length/editor and edition policy. Independent edition-publication
acceptance remains gated on #29–#31; service success does not complete those issues
or authorize a live release.

## Integration and closure audit — 2026-11-14

The historical base references above describe implementation order, not current
open PRs. All #50–#71 were retargeted to main and merged with their CI-tested
source SHAs after #72 isolated code integration from deployment. Deployment-policy
run 43604828727 passed. No queued/running legacy deployment job was found before
the merges; post-merge active jobs were hosted Application checks, not deployment.

Roadmap #1 checkboxes are reconciled with issue completion. Keep #5 open for real
storage validation, #19 for article-origin follow-through, #23 for independent
edition handoff, and #24 for remaining consumers. #27 onward requires its own
acceptance evidence. The GADS issue permits explicit reporting of unavailable
runtimes; a missing Claude CLI is a documented compatibility limit, not a claim
that the installed hook ran. No live Jarvis, data, account, volume or traffic
change occurred. See docs/operations/code-integration.md for release boundaries.

## Article-origin and sourcing implementation

Merged #74 provides article-specific origin assertions, immutable citation evidence
and safe anchors; 17 DB suites, three Chromium flows and all PR checks passed. #19
is complete. The citation-package branch completes #27 with versioned mappings,
private original-method evidence, attributed disagreements, explicit count
definitions and a sourcing publication gate. Validation and remaining limits are
recorded in docs/evidence/citation-policy.md. Publication web controls still wait
for #28/#29, and complete edition/publication/release acceptance remains separate.
