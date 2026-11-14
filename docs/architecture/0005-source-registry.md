# ADR 0005: explicit source rights and captured historical attribution

Status: selected for #17; private registry service only. No subscriptions, HTTP
fetching, live source changes or legal-permission certification.

Separate an outlet, a configured feed and a reporting origin. Outlet languages and
geographic coverage are independent operator declarations. Source-to-origin links
are configuration hints; they never create per-article origin assignments or
independent-reporting counts. Ownership/editorial notes are explanatory text, not
an objective trust/quality score. Reporting-origin provider identity is stable.

Publishers configure sources/feeds/origins; editors may read them. Reuse #16's
transactional live-session/role check, now a shared auth helper, instead of copying
HTTP-only checks. Every change has an expected row version and an immutable private
before/after audit attributed to the authenticated identity. Missing expected
version means create only, never upsert/overwrite. Delete means disable; no history
or citation deletion method exists. Disabling a source makes all its feeds inactive
without rewriting their own enabled flag or erasing previous references.

Storage and display permission are distinct. Rights status is unknown, documented
or original. Unknown always yields effective metadata-only storage/link-only
display, even when an old usage_policy claims a license. Preserve that old claim
rather than certifying or silently erasing it. New documented permissions require
public terms/evidence URL and a rights note; original rights require an explicit
ownership declaration. These are operator assertions, not a legal engine or proof.
robots.txt never grants permission. New database rows default disabled; an explicit
publisher configuration can enable them, without starting any fetch in this code.

Public citations formerly joined the live outlet name. Renaming an outlet would
rewrite historical attribution. Migration 6 captures the currently stored outlet
label on existing source articles and captures it on future inserts, then freezes
it. This snapshot is not a claim about the original branding years ago; original
legacy records remain available for reconciliation. Original URLs/source-article
identity are already immutable. Public citations now read the captured label.

Feed URL/parent and reporting-origin identity cannot be repointed in place: create
a new record and disable the old one. This also avoids races with an article being
attached while an operator changes identity. Notes, interval/size policy and enabled
state remain editable. Source names/ownership metadata are current configuration,
with changes audited; old citation labels stay captured.

Registry URLs must be bounded public HTTP(S) domain URLs, without userinfo, known
credential query names, IP literals or obvious local hosts. Public terms anchors
are useful; feed fragments are rejected. The validator performs no DNS lookup and
cannot prove arbitrary path/query values contain no secret. Operators must supply
only public URLs. Redirect/DNS/IP/port enforcement, timeouts and streamed-byte bounds
belong to #18/#19 and must be checked at each actual request. Registration performs
no HTTP request and does not provide a way to insert credentials into a URL.

Migration 6 preserves unrelated legacy/public objects and original source usage
claims. Runtime grants are column-scoped and tested with synthetic accounts; no
production role/permission change is made. No new crates are introduced.

Rollback before schema apply is source reversion. After apply, exact-version
readiness requires a matching binary/forward fix or verified restore. Do not drop
captured attribution or audit records to downgrade an existing archive.
