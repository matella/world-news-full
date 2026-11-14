# Structured article contract (#26)

ArticleFormat is a reading-length choice: brief, standard, analysis (displayed as
Brève, Article, Article long). ArticleKind separately declares information, analysis,
opinion, automated summary or explicitly historical summary/synthesis. Selecting
long format does not create an analysis claim or change authorship.

A revision stores title, optional standfirst/summary, a language tag, full body,
format/kind, classification and provenance. Explicit overview text is never generated
from a body substring. Repository reads round-trip the original strings and blocks.
No article text is padded, chopped mid-sentence or relabelled to fill a budget.

## Minimal plain block format

Body is a JSON array. The original paragraph/heading subset remains valid:

```json
[
  {"type":"paragraph","text":"Authored prose."},
  {"type":"heading","level":2,"text":"Authored heading"},
  {"type":"claim","id":"fact-1","text":"An explicitly identified factual claim."},
  {"type":"section","role":"context","blocks":[
    {"type":"paragraph","text":"Context explicitly entered by an editor."}
  ]},
  {"type":"section","role":"uncertainty","blocks":[
    {"type":"claim","id":"limit-1","text":"An explicitly attributed limitation."}
  ]}
]
```

Heading levels are 2 or 3. Section roles are context or uncertainty and occur at
most once each. Sections contain only paragraph/claim children and need nonempty
prose; recursive sections are rejected. A body allows at most 2,000 total blocks
including section children and 1 MiB total authored text; heading text is bounded
to 500 Unicode scalar characters. Existing separate request-size limits still apply.

Claim IDs are 1–64 ASCII letters/digits/hyphens/underscores and unique throughout
the revision, including section children. Render them as claim-{id} fragment IDs.
These are anchor identities only. Source binding, missing-reference warnings,
attributed disagreement and publication checks are #27; an anchor alone is not
citation evidence or automated fact verification. No absent reference is invented.

All text is plain. HTML-looking strings remain escaped visible text. Script/style
fields, arbitrary markup blocks, unsafe attribute IDs and unknown JSON fields are
rejected, with no sanitizer silently changing the source wording. Templates use
Askama escaping and controlled element/attribute names. There is no raw-HTML renderer,
external font/provider request or client script dependency in this contract.

## Reading and review behavior

The shared public/private body partial renders explicit standfirst text, an optional
native expandable En bref summary, the complete authored body, safe claim anchors
and optional labelled sections. French interface labels are separate from the
preserved article language. Summaries have keyboard-native interaction without JS.
The broader edition/article design and reader journey remain #34–#36.

For full rendering only, an overview string equal to body text after surrounding
whitespace is ignored in presentation. A summary equal to the standfirst is also
not displayed again. Stored fields remain unchanged. Case, wording and internal
spacing are not rewritten. An overview accessor for future edition/search consumers
returns only explicitly authored summary/standfirst; absent legacy fields stay absent.

Blank/heading-only drafts may be stored while editing, but entering review requires
nonempty prose. Word-count targets/overrides, significant-claim sourcing and full
editor controls remain #27–#29; structurally valid content is not an editorial-quality
certificate. This slice does not expose a publishing HTTP endpoint.

## Legacy and rollback compatibility

Legacy bodies map to ordinary paragraphs, including literal heading/HTML-looking
text. No standfirst, claim spans, context, uncertainty, new review or analysis is
inferred. Original language, payload hashes and historical pipeline metadata remain.
Original translation bodies using the paragraph subset remain readable as before.

New manual revisions record body_schema=2; old data retains its original provenance.
No SQL migration is needed because bodies already use JSON arrays. Older application
versions that know only paragraphs/headings can still fail on new variants. Before
activating rich-body writes in a real environment, rehearse compatible reader/rollback
images under #47–#49. Stop writes/retain a compatible reader if that gate is unmet;
do not flatten rich bodies, drop sections/claims or restore an old data snapshot
silently to accommodate an incompatible binary.
