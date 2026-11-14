# ADR 0021: private structured authoring with explicit saves

Status: accepted. Related: #29; extends ADR 0020.

Provide a server-rendered private draft board and editor using Axum/Askama and the
existing live-session editorial service. Native forms add blocks, preview submitted
content without persistence, or save an immutable private candidate. Preview shares
the escaped reader body/citation partials. This slice introduces no HTTP publication.

Flatten and compose plain structured blocks without changing authored strings or
claim identity. New claims receive random safe IDs; deletion requires an explicit
checkbox. Reject nonempty unused rows and repeated separated context/uncertainty
sections instead of silently discarding or reordering prose. Preserve original
CRLF bytes when browser-normalized submitted text is unchanged. A sentinel newline
in textareas preserves leading newlines through HTML parsing.

Use exact revision/version fences. Validation and conflicts return the submitted
fields for manual comparison; reauthorise before rendering recovery. No automatic
merge or autosave is implied. Text saves preserve lineage/classification through the
service, replace sourcing and require an explicit new length exception. Historical
kinds remain available only for genuine existing historical records.

Editor forms have a scoped 4 MiB streamed/request limit to accommodate encoded
bounded bodies; other routes retain 64 KiB. Reject malformed UTF-8, duplicates and
unknown fields. Roles, CSRF and Origin checks remain mandatory. Arbitrary story IDs
use /admin/editor/story/{id}, avoiding collisions with the new-article route.

No schema, dependency, source HTTP or production change. Existing binaries can read
the stored block model; rollback removes the authoring surface without deleting
history. Full source selection, publisher curation and the complete review/publish/
correct/withdraw interface remain #29 gates before exposing HTTP publication.
