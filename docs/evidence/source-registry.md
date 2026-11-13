# Source registry verification

Scope: #17, stacked on editorial PR #58. Internal source/feed/origin configuration
and additive migration 6. No real source setup, fetching, roles or deployment.

Passed locally:

- Affected-crate strict Clippy/build and 24 unit/in-process tests, including URL
  credential/local-target rejection without DNS.
- `python3 scripts/test_database.py`: all six fresh DB suites passed, plus real
  migration/import/auth/runtime checks. Existing auth/editorial suites still pass
  after sharing their live-session transaction helper with registry operations.
- Registry suite starts at v5 with a historical license claim, article/citation and
  feed, then upgrades. Raw claim remains licensed_text but unknown effective rights
  yield metadata/link only. No historical license/approval is invented.
- Public citation label survives outlet rename; a new article captures the new
  label. Changing captured attribution fails. Disabling an old feed preserves both
  original URL and captured label, without deleting source articles/citations.
- Real login sessions plus a restricted runtime role: editor can read but cannot
  configure; publisher can configure; public reader cannot read registry audit;
  runtime cannot DELETE sources or change accounts. Revoked publisher is denied.
- Unknown full-text and undocumented license configuration rejected. Documented
  internal licensed storage can remain link-only display. Credential-bearing URL
  is rejected before insertion. Configured languages/geography remain separate.
- Configured origin hint creates zero article-origin assignments. Identity changes
  require a new origin/feed. Source disable excludes its configured-enabled feeds;
  stale versions conflict. ID cursor lists are bounded.
- Missing relationship and injected audit failure roll back source/config/version
  and audit writes together. Audit records cannot be changed even by the test owner.

A preexisting test combined a missing source with unlicensed body text and depended
on constraint evaluation order. The new attribution trigger exposed that ambiguity;
it now separately verifies the body constraint against a valid source and the
missing-source FK. Neither invariant was weakened.

Not run/complete: actual HTTP fetching/DNS/redirect SSRF protections, extraction,
provider calls, admin screens, real rights review or production grants/deployment.
Source registry success does not establish copyright permission or a live pipeline.
All fixtures/accounts are synthetic, with .invalid URLs; no external source fetch.

Formatting, real-process outage/shutdown, foundation/secret/diff guards passed.
GitHub CI supplies full-workspace validation. See ADR 0005 and
source-registry.md for limits, credential handling and exact-version rollback.
No destructive down migration or historical data deletion is shipped.
