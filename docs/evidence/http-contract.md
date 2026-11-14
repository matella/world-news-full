# HTTP contract verification

Scope: #14, stacked on importer PR #55. R2 scoped source-only changes; no deployment,
authentication bypass, live DB or public routing change.

Passed locally:

- Affected-crate unit/in-process suite: 20 tests. Routes validate malformed dates,
  paths, duplicate/invalid queries and reject them before DB access. Tests cover
  relative slash redirects, unknown-path 404, unavailable-schema 503, truthful 501,
  read-only 405/Allow, HEAD, escaped masthead and server-generated request IDs.
- Real body overflow with no handler body extractor returns 413. Timeout drops a
  pending handler future; its drop guard confirms cancellation and a correlated
  503 response. Error pages have no secret/query/forwarded-host reflection.
- `python3 scripts/test_database.py`: all three explicit DB suites passed on
  separate fresh disposable databases (schema, importer and HTTP). The HTTP suite
  uses a reader with SELECT only on public views/schema metadata; base-table access
  fails. Empty content returns 200, absent content 404, closed pool 503.
- Imported synthetic title/body containing HTML are escaped. Source links and
  unverified legacy labels remain visible. Drafts are excluded. Cross-story revision
  IDs return 404. Canonical URLs ignore Host/Forwarded.
- Correction changes the current story, while an edition link still reads the old
  selected revision and labels it as retained history. Withdrawal removes both body
  URLs and leaves a marker in the immutable edition.
- The wrapper starts the real binary with its restricted reader role: root/archive
  200, missing story 404, invalid day 400, malformed stored content 503, search 501.
- `python3 scripts/test_rust_runtime.py`: real-process config/redaction, correlation
  headers, DB outage/unimplemented status and SIGTERM drain passed without DB or
  providers. Recorded logs exclude synthetic credentials and the search query.

Strict Clippy, formatting, affected-crate build and foundation/secret/diff guards
passed. GitHub CI supplies full-workspace checks after opening the PR.

No new resolved crates: http-body-util was already locked and used by Axum/tests;
its runtime use identifies body-limit errors. Dot-segment opaque IDs are now invalid
because browsers normalise them; import quarantines such IDs rather than creating
unusable links. Other IDs are preserved and encoded as individual URL segments.

Not run: complete browser/accessibility audit, new publishing journey, translation
toggle, complete public correction notices, search/topics or production performance.
The HTML is a minimal route/data renderer, not completed reader parity. See
`docs/http-contract.md` for limits and remaining issue ownership. PostgreSQL statement
timeout bounds server SQL; asynchronous HTTP cancellation is not instant SQL abort.

Rollback is reverting application changes. No migration or data rewrite is included.
Main and live traffic are unchanged; release remains gated by #47–#49.
