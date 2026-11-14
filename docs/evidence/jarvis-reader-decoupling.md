# Jarvis reader decoupling evidence (#23 slice 1)

Based on #67; source and runbook only, not a completed writer handoff. No Rust schema,
dependency, production account/grant/data, deployment, merge, writer contact/stop or
provider action is introduced. The existing legacy provenance marker remains.

The transitional Next reader removes the generated console links, ask URL helper,
private-LAN fallback and NEXT_PUBLIC_JARVIS_URL dependency. Its admin reader removes
the publisher status model/query and inferred processing/synthesis estimates. The
bounded publication list labels recorded historical data without inventing local
queue health or new authorship. Source text/IDs/URLs/counts/translations are unchanged.

Passed locally:

- `npm ci --prefix frontend --cache /tmp/worldnews-npm-cache` and frontend
  `npm run typecheck`, `npm run lint`, `npm test` (four tests), `npm run build`.
  Build used an explicit disposable/unreachable database URL, with telemetry off.
- `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_frontend.py`: synthetic HTTP,
  authenticated admin without news_status, live/archive/story browser navigation,
  preserved body attribution and mobile outage/retry checks passed.
- `npm audit --prefix frontend --omit=dev --audit-level=high`: zero production
  vulnerabilities. No dependencies/lockfiles changed; prior development findings
  are not represented as resolved by this production-only audit.
- `python3 scripts/check_foundation.py`, tracked-file secret guard and diff checks
  passed. These structural/pattern checks are not a complete security audit.
- Runtime-source scan: no Jarvis/config/LAN references in frontend/src. The Rust
  legacy_jarvis import provenance remains unchanged. Rust checks were not rerun
  for this source-only frontend/runbook slice; the preceding #67 checks apply to
  its unchanged Rust tree.

The HTTP fixture intentionally drops news_status
before visiting the protected admin route, proving it can read without that writer
contract. Live/archive/story reads retain the synthetic title/body, including an explicit
historical Jarvis attribution, and contain no generated analysis link or private-LAN fallback. Optional Chromium exercises those pages with
all external requests blocked and inspects links/requests. Existing empty/missing/
auth/read-only/outage checks remain in the wrapper.

The handoff runbook records inventory/staging, concrete release authorization,
final freeze/export/import/single-writer/traffic sequencing and rollback checkpoints.
After new Rust publication, reverting an old database can lose content; reverse
reconciliation is explicitly unverified rather than presented as a safe reset.

Remaining #23 acceptance: independent new-edition creation/review/publication with
Jarvis unreachable depends on #26–#31 and #48. Real archive reconciliation, actual
live writer ownership/control, backup/restore, production grants and the authorized
handoff remain open under #47–#49. Do not close #23 or claim Jarvis has been stopped.
Codex and Claude Code must preserve these gates and the historical import metadata.
