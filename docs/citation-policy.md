# Citation and attribution policy (#27)

`RevisionInput.evidence` is optional draft data. `SourcePackage` has a statement,
bindings, firsthand methods and disagreements. `EditorialService::source_revision`
requires a live editor, exact head version and reason; it creates a new reviewable
revision without rewriting authored text, classification or original provenance.
It replaces citation mappings rather than guessing mappings from old claim JSON.
Initial creation or a text revision can supply an explicitly authored package too.

## Input contract

- Statement: mode `external`, `original`, `opinion` or `historical`; public
  attribution (1–2,000 characters) and optional public limitations (same bound).
- Binding: source article ID, optional exact body claim ID and optional plain
  source location (1–2,000 characters). URLs come from the immutable source record.
  Missing anchor means a general reference, never a claimed precise mapping.
- Original method: observation, interview or document review; public attribution,
  selected claim IDs and private evidence note. An empty claim list covers whole
  prose only when the body has no formal claim anchors. It cannot silently attest
  other anchored claims. No human evidence is synthesized from intake text.
- Disagreement: explicit subject and 2–20 distinct attributed positions. Each
  position identifies an actual body claim with a citation or firsthand method.
  The software does not invent opposing positions or decide which one is true.

At most 200 unique source/anchor bindings, 50 original methods, 20 disagreements,
2,000 claim IDs per method and 256 KiB serialized package. Private notes are bounded
to 10,000 characters each; public position attribution to 1,000. Unknown fields,
duplicate bindings/positions, missing anchors and invented source IDs are rejected.
A failed source insert rolls back the whole candidate/head change.

## Publication gate

Approval refers to one exact revision. Publication revalidates a live publisher,
approval, frozen source versions and the package after taking evidence locks. It
requires an attribution statement and evidence for every declared factual claim.
External reporting requires actual claim/source mappings. Original reporting needs
a documented human method and private evidence. Opinion mode matches opinion kind.
One source is sufficient when its limits are explicit; uncertain/contested origins
also require a public limitation note. There is no ten-source quota or truth score.

Historical mode is limited to preserved imported Jarvis lineage and historical
article kinds, with explicit unverified limitations. It permits transparent archive
metadata corrections, not relabelling newly written reporting as a legacy import.
Existing archived publications remain readable with incomplete attribution warnings.

Draft/review can retain missing evidence so editors can finish work. The private
draft page displays missing declarations/anchors and general/unmapped references.
The software cannot identify every material factual sentence: selecting anchors,
evaluating original evidence, fairness and adequate context remain human review.
The full editing/publishing web workflow remains #29; no publishing endpoint is
enabled by these service changes. Word budgets are the next publication gate, #28.

## Reader definitions and privacy

The shared public/private report displays reference footnotes, source passages,
captured origin attribution, public limitations, original methods and linked
disagreement positions. Plain source text remains escaped. Counts distinguish:

- Documents: distinct URLs under existing URL-v1 rules, removing known tracking
  parameters while preserving routing fragments and unknown query distinctions.
- Media: distinct declared source registry records, not certified legal ownership.
- Documented reporting origins: distinct attributed origin IDs across consistent
  document assertions. Shared dispatches count once. Unknown/contested documents,
  including inconsistent assertions for one canonical document, do not add certainty.

Counts describe attribution and do not guarantee independence or factual truth.
Historical snapshots retain their original versions, with a warning if current
registry evidence differs. Private notes and actor identities never enter the public
view/template. The SQL projection allowlists nested fields, including disagreement
positions; adding future private fields cannot accidentally expose them wholesale.

## Verification and release

`WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`
adds the disposable citation suite and anonymous browser. The new publisher read
path needs SELECT on citations/source articles/origin labels/origin registry and
EXECUTE on `news.lock_citation_evidence(text)` alongside existing workflow grants.
This is source-level role design, not approval to change a live role. Public readers
keep their existing public-view grants; workers gain no authority. The pure JSON
projection is not SECURITY DEFINER. See ADR 0018 and the linked evidence record.
Never deploy an older writer that bypasses the new sourcing gate.
