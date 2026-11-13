# ADR 0008: Rights-scoped extraction and independent manual drafts

Status: accepted for staged #20, based on #61. No deployment.

Feed and manual source metadata do not grant text storage or republication rights.
Authenticated editors may request extraction only when a source has valid explicit
licensed/original storage evidence. Unknown or inconsistent declarations are denied
before network access. The article hostname must equal the configured homepage
hostname; the first article origin then bounds every redirect hop. This deliberately
rejects alternate/CDN domains, HTTP-to-HTTPS redirects and ambiguous pages until an
explicit reviewed scope model is justified. It never expands rights based on a link.

Use existing SafeHttp DNS/IP/TLS/redirect/time/byte limits with a new same-origin
variant. Extraction performs no authentication, cookies, JS, AMP/cache substitution,
archive lookup, JSON-LD body recovery or paywall workaround. Explicit paywall markers
or isAccessibleForFree=false fail closed. There is no universal access-control or
visibility detector: CSS from external stylesheets and site-specific access rules
need source review. A successful HTTP response alone does not certify permission.

Parse bounded UTF-8 text/html using scraper 0.29.0 (ISC), aliased html_scraper to avoid
the existing workspace scraper mock's name. Default CLI/error features are disabled.
A proper HTML5 parser decodes entities and traverses elements without executing
content; regex stripping would be unsound. The lock adds 23 packages (html5ever,
selectors/CSS parsing, ego-tree, string interners and support/proc macros). These
have ISC, MIT/Apache-2.0 and MPL-2.0 licenses. Unmodified third-party crate licenses
must remain in distribution notices when packaging (#46). No existing locked
version is upgraded. Local compilation establishes compatibility with Rust 1.91;
not all dependencies declare an MSRV. This is not an advisory/security audit claim.

Require a single visible article or, if absent, main boundary; preserve visible p
paragraphs in document order. Retain an h1 title, author metadata and RFC3339 source
publication timestamp when present. Strip markup/active subtrees/explicitly hidden
content. Inert text can still contain prompt injection or literal angle brackets;
escape on rendering and never treat text as instructions. No arbitrary whole-body
fallback or silent truncation. Unsupported/ambiguous/oversize pages need manual intake.

Extraction takes a live-session/source/article snapshot, releases all locks for
network/parser work, then revalidates session, rights/source version, URL and expected
intake version under locks before commit. Concurrent work may fetch twice; only one
completion for a version commits. Jobs/leases/recovery remain #21, not a hidden loop.
Identity, public citations and the original article title/body are unchanged.

Private snapshots preserve metadata, actor, policy evidence, hashes, status and
provenance. Publisher paragraphs live separately in intake_text: a source storage
revocation removes that text transactionally while retaining immutable metadata and
an automatic policy-removal record with source version. Re-enabling rights does not
restore removed text. Direct DB maintenance remains privileged; application role
reads also recheck current storage rights. This supports rights withdrawal without
making the audit history itself a permanent copyrighted-body store. Backup retention
and deletion obligations still need the source's actual terms/release runbook.

Manual link registration and editor-authored notes need no text license or network.
OriginalDraft is a separately authored revision; no extracted text is copied into it.
Draft creation, source-to-initial-revision link and notes are atomic using the existing
editorial transaction helper. This is a provenance link, not claim-level citation or
verified reporting origin. No publication/review bypass or HTTP editor endpoint.
