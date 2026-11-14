# World News project rules

Read `docs/architecture/current-state.md`, `docs/architecture/0001-rust-publishing.md`,
`docs/editorial-policy.md` and `docs/implementation-plan.md` for current versus target
behaviour. This project-owned file can change; do not hand-edit GADS-managed files.

## Stack and ownership

Current: Next.js reads Postgres; Rust gateway/scraper/AI are mocks. Target: Rust
Axum + SQLx/Postgres + Askama, one web application and optional worker. No required
Node, Jarvis or AI provider in the final app. The app owns migrations and publication.
Preserve IDs, source links, languages and archive data; imported schema is unknown
until inspected. Do not invent migrations from TypeScript casts.

Public reading is anonymous. Admin/writes require server-side authorisation.
Editorial services revalidate a live session/role inside each transaction; never
trust a caller-provided actor or a cached role for a privileged state transition.
Any revision change invalidates review. Future citation/length changes must also
invalidate the affected approval or create a new reviewable revision. Do not expose
a production publishing endpoint before citation/length/editor acceptance is met.
Intake/AI -> draft -> review -> publication. No automatic publication. Preserve
revision/provenance and transparent corrections. Configurable article budgets live
in `config/editorial.json`; do not pad or truncate to hit them. Treat feeds/text as
untrusted data. Do not conflate language, geography, outlet count and reporting origin.
Registry relationships are hints, never verified article origins. Unknown rights
must resolve to metadata/link-only behaviour even when a legacy policy claims more.
Use effective policies in future fetch/extraction/display paths; registry URL
syntax checks do not replace DNS/redirect/IP checks at actual fetch time. Preserve
captured source attribution and disable configuration instead of deleting history.
Use the shared SafeHttp policy for future extraction, not an unguarded new client.
Keep TLS verification, every-hop DNS/IP checks and byte/time bounds. No body/token/
URL payload logging, production private-address escape or ambient credential proxy.
Preserve normalization observations and original links; never use a headline or parser
ID as an article identity. Keep changed metadata for review without rewriting existing
articles. Configured origin links do not certify article origins.
Only extract within documented source storage rights and an origin-scoped SafeHttp
request; recheck rights/session/version after network work. Keep retained publisher
text separate from immutable provenance so revoked storage can remove the text.
Manual notes/original drafts are separately authored, never automatic text copies.
Persist metadata before advancing feed validators; fetch status is not publication
or whole-pipeline success. Do not add a startup polling loop before job/recovery gates.
Use separate worker DB authority, never a fabricated/permanent editor session.
Normalization jobs must fence writes and acknowledge success in the same transaction
as their content receipt. Preserve attempts/generations and audit manual replay.
The explicit normalization worker does not authorize fetch/extraction scheduling.
Machine extraction requires a one-hour publisher grant with current issuer/rights/
version/lease checks. Snapshot origin is human actor XOR worker job; never fabricate
a human actor or persist a session for machine work. Share source retry minimums.
Feed completion must hold a live attempt fence and atomically enqueue normalization
before cache advancement. Recover abandoned attempts explicitly; never reset leases
silently. Preserve Retry-After minimums and unknown historical holds across resume.
The combined worker is explicit, budgeted and capped; feed IDs are an exact bounded
JSON allowlist and extraction requires opt-in plus publisher grants. Interrupted
reports are partial observations, never pipeline health or publication success.

Private pipeline administration must use the editor pool and live service/session/
role/version checks. Keep lists bounded and source values escaped. Publisher retry/
pause/configuration actions never execute source HTTP or publish. Source text storage
removal requires a concrete versioned confirmation; manual notes/original drafts
remain distinct from retained publisher text. Do not widen public reader authority.

Reader Jarvis decoupling does not authorize a live writer stop. Preserve historical
legacy_jarvis provenance and article data; never relabel it as newly team-authored.
Follow docs/jarvis-handoff.md for inventory, exact release artifacts, authorization,
final reconciliation and single-writer/rollback gates. Independent edition-publication
acceptance remains blocked on its editorial/edition policies, not hyperlink removal.

Language never determines geographic relevance. Use config/taxonomy.json and the
shared Rust projection for classification/section consumers. Keep the transitional
generated JSON byte-identical (check_foundation enforces drift). Legacy mappings
are proposals; preserve raw payloads/frozen snapshots and explicit unknowns. Editor
classification changes create a versioned candidate, retain prose/citations/original
provenance and invalidate review; do not transfer old translation/review approval.

Threads are explicit primary subject identities, separate from multi-valued topics.
Never auto-merge legacy related IDs or similar titles. Curation requires a live
publisher, versions and reason; preserve before/after history and exact-state undo.
Keep unknown update times unknown, and reader heads/counts limited to public members.
Use the bounded public views/resolver and repeatable timeline snapshot, not private
tables or separate racing resolve/member reads. See docs/story-threads.md; editor/
reader consumers remain their own gates. Never delete identity/history for rollback.

Article bodies use plain bounded JSON blocks, never raw HTML authority. Preserve
explicit overview fields and original-language prose; suppress duplicate display
without rewriting stored content. Context/uncertainty and claim IDs must be explicit,
unique and validated. Do not infer them from legacy text or treat anchors as cited
facts. Share the escaped public/private body partial. Rich-block writes require
compatible reader/rollback images even without SQL migration; never flatten/delete
new blocks to accommodate an old binary. See docs/article-structure.md.

Article reporting-origin evidence is publisher-authored and article-specific;
registry links never certify it. Preserve sealed versioned assertions, private notes
and captured citation evidence; classification edits copy the exact origin versions.
Do not guess legacy anchors or turn outlet counts into independent-reporting counts.
Public views must exclude private evidence/actors. See docs/article-origins.md and
docs/citation-policy.md. Source packages belong to immutable revisions: source-only
changes clone prose/provenance and reset review; text edits never inherit sourcing
implicitly. Publication locks/rechecks evidence and requires explicit attribution,
covered factual anchors and public limits for single/uncertain-origin reporting.
Original methods need real human declarations/private evidence, never manufactured
external sources or automatic attestations. Preserve explicit historical lineage.
Public projections allowlist nested fields; do not serialize private packages.
Counts distinguish canonical documents, declared outlets and documented origins,
not truth or guaranteed independence. Workers receive no new curator authority;
word budgets/editor web acceptance remain #28/#29 and live changes their release plan.

## Commands and evidence

| Purpose | Command from repository root |
| --- | --- |
| Foundation contract/GADS drift | `python3 scripts/check_foundation.py` |
| Full GADS audit | `python3 ../gads-source/scripts/repo_audit.py .` after pinned checkout |
| Transitional frontend | `cd frontend && npm ci && npm run typecheck && npm run lint && npm test && npm run build` (explicit DATABASE_URL required) |
| Current Rust skeleton | `cargo build --workspace --locked` |
| New Rust foundation | `cargo run -p world-news -- --check-config`, `cargo run -p world-news` (DATABASE_URL required; default port 3001) |
| Taxonomy / sections | `NEWS_TAXONOMY_CONFIG` optional bounded Rust policy; see docs/taxonomy.md; offline import reports proposed mappings only |
| Protected pipeline UI | `/admin/pipeline` and source/feed/article/job forms; see docs/pipeline-admin.md; `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py` adds real Chromium workflow |
| Bounded intake orchestration | `world-news --work-intake-once`; default no feed HTTP/extraction; typed allowlist/caps/budget in docs/intake-worker.md |
| Explicit selected feed / recovery | `world-news --poll-feed-once ID` (can fetch) / `world-news --recover-feed-leases-once` (no HTTP); see docs/feed-recovery.md; no startup scheduler |
| Explicit normalization worker | `world-news --work-normalization-once` with an isolated worker DATABASE_URL; see docs/normalization-jobs.md; no startup scheduler |
| Rust process smoke test | `python3 scripts/test_rust_runtime.py` after cargo build |
| Owned schema integration | `python3 scripts/test_database.py` (creates a disposable local DB; regular cargo test deliberately ignores this opt-in suite) |
| Explicit schema change | `cargo run -p world-news -- --migrate` with authorised migration-owner DATABASE_URL; never run against live data without its release plan |
| Offline import plan | `cargo run -p world-news -- --import-legacy FILE --dry-run`; no DB URL/network; review JSON report and exit 2 quarantine |
| Staging import apply | `cargo run -p world-news -- --import-legacy FILE --apply` after migration; no live data without the explicit cutover/backup plan |
| Staging local account bootstrap | `world-news --create-admin USER editor\|publisher` with authorised operator DB URL and password stdin; see docs/admin-auth.md; never automatic/live provisioning |
| Staging account revocation | `world-news --disable-admin USER` with authorised operator DB URL; disables identity and all its sessions |
| Rust checks | `cargo test --workspace --locked`, `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings` |
| Persistence drill | `python3 scripts/test_persistence.py` (disposable Docker only) |
| Frontend integration | `python3 scripts/test_frontend.py` after build; optional `WORLDNEWS_BROWSER_CHECK=1` uses installed Playwright/Chromium |
| Narrow secret-pattern guard | `python3 scripts/check_secrets.py` (does not scan history or prove absence of secrets) |

Update these commands as implementation lands. Never report planned checks as
passed. Use synthetic fixtures and disposable databases. Separate unit/build checks
from complete publishing/browser/recovery evidence. Scoped PRs must link issues,
acceptance criteria, commands/results, limitations and rollback implications.

## Agent adapters

Codex reads AGENTS.md, then this file explicitly; CLAUDE.md imports apply to Claude
Code. `.claude/settings.json` configures the installed guard and rendered roles.
Claude's guard does not protect Codex; each runtime's sandbox/approval settings
remain authoritative. Roles are available, not a requirement to spawn agents.
Document runtime tests honestly: installed files and unit tests do not prove live
agent instruction discovery/enforcement. See `docs/gads-adoption.md`.

## Authorised work and release boundary

The owner requested execution of roadmap #1. Scoped source edits, dependencies,
local tests, commits, feature-branch pushes, PRs and issue progress updates are in
scope. The owner explicitly authorised merging validated PRs and progressively
closing completed issues. PR #72 retired both push-deployment workflows before
#50–#71 merged; see docs/operations/code-integration.md. Merge scoped PRs into main
with passing relevant checks and expected head SHAs, then close only completed
acceptance criteria. Source integration does not authorise a live release. Keep
the manual release hold until #46–#49 and concrete environment approval are met.
Do not stop a live
Jarvis writer, switch traffic, deploy, publish packages/images, modify live secrets,
copy production records, incur provider costs or delete production data without
explicit action/environment-specific authorisation. Prepare and verify locally
first; approvals must concern concrete changes, not speculative future work.
