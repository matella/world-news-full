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

## Commands and evidence

| Purpose | Command from repository root |
| --- | --- |
| Foundation contract/GADS drift | `python3 scripts/check_foundation.py` |
| Full GADS audit | `python3 ../gads-source/scripts/repo_audit.py .` after pinned checkout |
| Transitional frontend | `cd frontend && npm ci && npm run typecheck && npm run lint && npm test && npm run build` (explicit DATABASE_URL required) |
| Current Rust skeleton | `cargo build --workspace --locked` |
| New Rust foundation | `cargo run -p world-news -- --check-config`, `cargo run -p world-news` (DATABASE_URL required; default port 3001) |
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
scope. Do not push to main: its current workflows deploy on push. Do not merge
until a concrete release effect is understood and authorised. Do not stop a live
Jarvis writer, switch traffic, deploy, publish packages/images, modify live secrets,
copy production records, incur provider costs or delete production data without
explicit action/environment-specific authorisation. Prepare and verify locally
first; approvals must concern concrete changes, not speculative future work.
