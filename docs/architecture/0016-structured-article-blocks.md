# ADR 0016: Plain structured article blocks and explicit overview fields

Status: accepted for #26, based on #70. Staged, not deployed.

Keep headline, standfirst, summary and full body independent. Do not manufacture a
short overview by slicing prose or infer analysis/uncertainty from a legacy body.
An explicit overview accessor prefers summary then standfirst. Full rendering omits
an identical overview/body paragraph or duplicate standfirst/summary presentation,
while preserving the exact stored fields and body. Equality ignores surrounding
whitespace only; no semantic rewrite, padding or truncation is performed.

Extend the existing JSON paragraph/heading subset with claim paragraphs and explicit
context/uncertainty sections. Sections contain paragraphs/claims only; no recursion,
arbitrary HTML, styles, links, images or executable blocks. Claim IDs are unique
across the body and safe for prefixed fragment anchors. Context/uncertainty roles
are unique and require authored prose. Bound total nested text/blocks and headings.
Raw HTML-looking text remains literal data, escaped by Askama rather than stripped
or interpreted. This preserves original wording without granting markup authority.

Use one escaped article-body partial for public pages and private draft previews.
Render authored headings, stable claim anchors and only explicitly entered optional
sections. Distinct summaries use native keyboard-operable details without a script
runtime. Preserve article language; French interface labels have their own language
annotation. Format length and article kind are separately labelled: a long reporting
piece is not automatically analysis/opinion. Heading-only drafts cannot enter review.

New manually submitted revisions record body_schema=2 in provenance. The v1
paragraph/heading subset remains readable unchanged, including legacy/imported and
translated text. No schema migration is required by JSON storage. New variants are
nevertheless a data-format compatibility change: an older reader may reject them.
Before live writes, release/rollback images must support the format; never flatten
or delete new blocks silently to make an old image accept them.

Claim anchors do not fabricate citation bindings or certify facts. Citation/source/
disagreement integrity is #27, counting/overrides #28, full editing #29 and complete
reader presentation #35/#36. The public publishing HTTP gate remains closed until
those policies are implemented. No provider or automatic publication is required.
