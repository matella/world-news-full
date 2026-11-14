# Private article authoring

Open `/admin/editor` after signing in with an authorised editor/publisher account.
The bounded board filters candidates, review, publications and withdrawals, with
keyset pagination. `/admin/editor/new` creates a draft; existing stories use
`/admin/editor/story/{id}`. Authentication remains disabled unless configured.

Choose format, article nature and language, then write overview fields and plain
paragraphs, headings or claims. Blocks can belong to a context or uncertainty
section. Add a block without saving; explicit deletion removes a row. Claim IDs
survive title/body edits and newly added claims receive safe generated IDs. There
is no raw HTML or JSON authoring authority. Overview and authored prose stay distinct.

Preview renders submitted fields through the reader body and citation templates,
with configured length guidance. It does not save or attach evidence. Save creates
a private revision and redirects to its edit page. Sources must be attached again
after a prose edit; a previous length exception is not silently inherited. The
existing draft detail page provides classification and length controls. Historical
records retain their historical identity and original import lineage.

A private revision reason is required for corrections. An optional public length
exception is explicitly labelled public. Stale saves return HTTP 409 and the
submitted text for manual comparison with the stored version; validation returns
400 with recoverable fields. There is no automatic merge or durable local autosave.
Closing the page before saving can lose unsaved input.

Forms accept at most 4 MiB only under the editor paths; domain block/text bounds
still apply. Malformed encoding, duplicate/unknown keys and publication intents are
rejected. Other routes retain their existing limits. All mutations require the
existing live session, role, Origin and CSRF boundary. No action in this form publishes.

See ADR 0021 and docs/evidence/editor-authoring.md. Source-package selection,
article-origin/thread controls and full publication workflow remain #29 work.
