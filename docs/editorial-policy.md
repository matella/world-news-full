# Editorial policy v1

Related issues: #9, #26–#33. Executable settings: `config/editorial.json`.
These are selected initial settings, not claims about existing publisher behaviour.
The Rust editor/domain will enforce them; the current Next app does not yet do so.

## Publication identity and formats

Serve French-reading general readers with Belgium/world coverage. Publish
manually when an editor has a complete edition; do not promise a daily edition
until operations can sustain it. Group dates in Europe/Brussels, store instants
in UTC, and label the latest available edition honestly when it is older than today.

| Format | Initial body budget | Editorial purpose |
| --- | --- | --- |
| Brief | 80–150 words | One development, essential facts and attribution |
| Standard | 350–600 | What happened, evidence, context, why it matters |
| Analysis | 800–1,200 | Supported explanation and deeper context; clearly labelled |

Budgets are configurable guidance. A reasoned override is preferable to padding
or mechanical truncation. Count body prose/headings; exclude headline, standfirst,
source lists and citation markers. Supported-language tokenisation must be
documented/tested in #28. Reading estimates initially use 200 words/minute and
round up; label them estimates. A 10-minute edition target is a composition
warning, never a reason to omit essential caveats or fabricate filler.

## Content semantics

- Headline: accurate statement, not a stronger claim than the sources support.
- Standfirst: short introduction; summary: optional concise overview distinct
  from the full body. Neither is silently manufactured from missing legacy data.
- Body: plain structured paragraphs/headings/claim anchors; explicit context/uncertainty
  sections only when useful. See [block contract](article-structure.md).
- Reporting, analysis, opinion and automated summary are distinct labels.
  Analysis is not opinion; opinion needs explicit attribution. Legacy synthesis
  is not automatically independently verified reporting.
- Cite significant factual claims to source records; mark attributed disagreement
  and uncertainty. Multiple outlets copying one dispatch are one reporting origin.
- No generic ten-source claim, invented citation or automatic truth score.

## Review and correction rules

Draft -> review -> published. Intake/AI suggestions always enter draft. A publisher
reviews sourcing, format/length, classification, language and unresolved warnings.
Store who reviewed which revision and when. Reject stale concurrent edits.
Published revisions are immutable; correction/withdrawal creates a reasoned event
and reader notice. Edition snapshots preserve selected revisions/order while
exposing later notices. Restrict internal notes to editorial access.

Translations identify language/method/review status and original revision. An
original revision change makes translations stale until reviewed. Show untranslated
fallback honestly. Geography and topic are independent of language. Do not derive
Belgian relevance from French/Dutch alone. See [taxonomy rules](taxonomy.md) and
config/taxonomy.json for explicit multi-tag classification, precedence, unknowns
and revisioned editor overrides.

## Acceptance examples

`fixtures/editorial-examples.json` contains synthetic brief, standard and analysis
examples within the default budgets. Repeated facts from syndicated copies must
not inflate coverage/length. An 800-word draft with one unverified rumour still
fails review; a 65-word urgent notice may pass with a documented length override.
Imported text is untrusted material, not instructions. Rights metadata determines
whether full text may be retained/displayed; a feed is not permission to republish.

See #28 for counting/overrides, #27 for citations, #29 for editorial UI and #30
for edition composition. Software validates structure; editors assess relevance,
fairness, adequate context and whether the evidence supports publication.
