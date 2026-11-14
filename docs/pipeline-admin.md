# Protected pipeline administration

This staged Rust interface implements #22 after #66. Start the web process with
NEWS_ADMIN_ENABLED=true, a separate public DATABASE_URL, EDITOR_DATABASE_URL and
PUBLIC_URL as documented in admin-auth.md. Migrate with an authorized migration
owner first; migration 13 adds only query indexes. No new account, database role,
source subscription or scheduled worker is created by startup.

The editor role needs explicit private read/intake/editorial grants appropriate to
these services. The synthetic integration-test grants demonstrate separation, not
a production provisioning script. Existing identities and sessions remain required;
never use the migration owner as the application or public reader identity.

| Page | What it shows or permits |
| --- | --- |
| /admin/pipeline | Actual source/feed/article/queue counts, paused feeds, last successful feed check |
| /admin/sources | Bounded source list; publisher source creation |
| /admin/sources/{id} | Rights/configuration, source documents, publisher edit/disable and add-feed forms |
| /admin/feeds | Bounded configuration/status list, age of successful checks; stale means older than two configured intervals |
| /admin/feeds/{id} | Configuration, attempt history and audited publisher pause/resume/disable |
| /admin/articles | Private documents, source/state filters, keyset pagination |
| /admin/articles?source={id} | Register a source link and original notes under the selected source |
| /admin/articles/{id} | Captured attribution, history, original link, own notes, original-draft creation; rights-eligible publisher extraction authorization |
| /admin/articles/{id}/snapshots/{snapshot} | Rights-gated retained text or original notes; withheld state after rights removal |
| /admin/jobs | Normalization status, attempts/quarantine, audited publisher replay/cancel |
| /admin/extractions | One-hour grant/job status and audited publisher cancellation |
| /admin/batches | Committed batch metadata, normalization handoff or publisher enqueue |
| /admin/drafts/{id} | Read-only original private draft, never a publication action |

All lists are bounded to twenty records with keyset links. Missing records return
404; stale form versions 409; invalid input 400; denied roles/CSRF 403; unavailable
private persistence 503. Errors omit SQL, credentials and source payloads. A feed URL
is an immutable identity: add a new feed for another URL. Editing does not reset
attempts, cache, retry minimums or source provenance.

Unknown source rights default to metadata/link-only storage; extraction is withheld.
Manual links/notes and empty original-draft forms remain usable without source HTTP
or AI. Draft format is an editorial length choice, not a change of article kind.
Separate paragraphs with a blank line. Full structured editing/review/publication
and citation/length checks remain subsequent roadmap work.

Changing a source from retained text to metadata requires the explicit removal
confirmation. The first submission does not change the source or delete text;
confirmation rechecks the version and publisher session. History, original links,
notes and separately authored drafts survive the retained-text removal.

Operations enqueue/authorize work but do not execute it in the HTTP request. Use the
explicit bounded worker commands with their separate worker authority. No scheduler
or source fetch is enabled by visiting these pages. Stop serving the staged feature
branch to roll back the UI; retain durable records and audit history.

Verification: build world-news, then run WORLDNEWS_BROWSER_CHECK=1 python3
scripts/test_database.py with installed Playwright and Chromium. This creates and
removes isolated local Postgres databases, exercises permissions/HTTP transactions,
and performs a real browser source/link/original-draft workflow without external
requests. See evidence/pipeline-admin.md for the recorded outcome and limits.
