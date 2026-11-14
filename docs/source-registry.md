# Source registry service

Issue #17. Registry is an internal configuration service, not an ingest worker or
admin UI. No URL is fetched and no feed subscription is started by these methods.
See ADR 0005 for identity, rights and attribution decisions.

## Source, feed and origin records

SourceSpec records name/homepage/enabled, ownership/attribution/editorial notes,
languages and geographic coverage, storage policy, terms URL, rights status/note,
display policy and configured origin IDs. OriginSpec records a stable named
reporting origin and optional provider source, note and enabled state. FeedSpec
records a stable parent/URL, enabled state, poll interval and response-size cap.
None has an actor/role input, numeric quality score or credential value/reference.

`Registry::save_source/save_feed/save_origin(session,id,expected,spec)` requires a
live publisher session. expected=None creates only; Some(version) updates only.
Conflicts require review/reconciliation, not a forced version refresh. The service
records the complete before/after configuration privately and atomically. Editors
can read single records and bounded ID-cursor lists (PageSize 1–100).

Feed URL/parent and origin identity are immutable after creation. For a correction,
create a new record and disable the old one. Notes/limits/enabled state can change.
`disable(session, Entity, id, expected)` is logical retirement; it remains available
for malformed historical configurations without revalidating an unsafe old URL.
No records, source articles, source-origin assignments or citations are deleted.
Active feed listing requires both source.enabled and feed.enabled. It represents
configured eligibility at that read, not proof of freshness or cancellation of a
future/in-flight fetch. Workers must recheck policy/disabled state at execution.

Source-origin links are configured hints only. They do not assign an origin to an
article, count independent evidence or merge a story/thread. An origin can be shared
by multiple outlets without inflating reporting independence. Languages do not
imply geography; neither is inferred or automatically classified here.

## Rights and content limits

| Status | Effective behaviour |
| --- | --- |
| unknown | Metadata/link only, including old rows with an unverified licensed_text/original claim |
| documented | Chosen storage/display permissions require terms URL and explanatory note; full-text display needs licensed-text storage |
| original | Original storage needs an explicit ownership note in rights_note; chosen display is recorded separately |

Unknown means unknown. Terms URLs and notes are operator declarations, not a legal
verification result. robots.txt is not a content license. No paywall bypass, bulk
subscription, raw article extraction or copyrighted full-text redistribution runs.
Future extraction/display paths must use effective policies and explicit per-article
rights decisions; source configuration does not rewrite an existing article's
stored usage policy or automatically expand its permission.

Names are 1–300 characters; notes <=10,000 and attribution <=1,000. At most 20
languages/coverage labels/origin hints; coverage labels <=100 characters and
language tags validated independently. Duplicate origin IDs fail. Feed polling is
60–86,400 seconds; response cap 16 KiB–10 MiB (2 MiB default). Future workers must
enforce that cap while streaming, not simply trust Content-Length.

URLs are <=2,048 bytes, HTTP(S), domain-based, no userinfo/IP literal/obvious local
host or known credential query names. Terms anchors are allowed; feed fragments
are not sent over HTTP and are rejected. Safe-looking domains can still resolve or
redirect to private hosts: this syntax check is **not** fetch-time SSRF protection.
Do not put signed/access-token URLs, secret query/path data or credentials in any
registry field. The validator cannot detect every arbitrary secret; use only public
URLs. Private credentials need a separately reviewed future binding mechanism.

## Historical integrity and private access

Source article insertion captures the current canonical outlet label. Migration
backfill captures the label stored at migration time, not an invented original
publication-time label. Once captured it cannot change; public citations use it.
Renaming/disabling an outlet or feed keeps old source IDs, original links and
captured labels. Legacy raw records keep the original payload for reconciliation.

The registry runtime needs the auth runtime grants, SELECT/INSERT on sources,
feeds, reporting_origins, source_origin_links; column UPDATE for editable config/
lock_version; DELETE only on source_origin_links for atomic configuration replacement;
INSERT registry_actions and USAGE registry_actions_id_seq. It cannot delete sources/
feeds/origins, update/delete audit, control accounts or own schema. Public readers
receive none of these tables, only existing public views. The disposable test role
proves these boundaries; no real role grants are provisioned here.

Tests cover v5 -> v6 with old claimed rights/attribution, strict session permissions,
version conflicts, source/feed disable, origin hints versus article assignments,
rights validation, secret URL rejection, preserved historical citations, captured
new labels and transaction rollback. `scripts/test_database.py` runs the six fresh
DB suites explicitly; ordinary cargo test intentionally ignores DB-dependent tests.

No new dependency or network service is introduced. Admin screens, resilient fetch,
rights-aware extraction and job execution remain #18–#22. Production release and
backup/rollback remain separate gates; keep a matching schema-version binary.
