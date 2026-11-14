# Article length and reading estimates

The default body ranges are 80–150 words for a brief, 350–600 for a standard article
and 800–1,200 for a long/analysis format. Format and article kind remain separate.
The editor can justify an exception instead of adding filler or cutting the text.
No operation in this policy changes prose to meet a number.

## Counting policy `word-v1`

Count Unicode letter/number runs in authored body paragraphs, headings, claims and
section children. NFC-normalize only the count input; preserve stored strings. An
internal straight/curly apostrophe or ordinary/nonbreaking hyphen joins a word:
`l’été`, `c'est`, `franco-belge` and `COVID-19` each count once. En/em dashes separate
words. Decimal/grouping dots or commas and numeric nonbreaking/narrow spaces between
digits join numbers: `1.234,50` or `1 234` counts once. Ordinary spaces separate
tokens; punctuation-only text, emoji and isolated combining marks add no word.

Authored heading text counts. Headline, standfirst, summary, generated interface
section labels, JSON keys, claim IDs, reference lists and generated footnote markers
do not. HTML-looking literal prose remains plain text; it is not stripped as markup
or granted rendering authority. Each block is counted separately. Count alone does
not identify factual significance or certify relevance, fairness or quality.

This rule is tested for French, English, Dutch and German (including language
subtags). Other languages have an explicitly approximate lexical-unit count; the
software does not pretend to segment Chinese or certify an unknown language. When
configured, an editor must justify use of that approximate count. The reader sees
no numeric reading-time estimate for an unsupported counting language.

## Configuration

`NEWS_EDITORIAL_CONFIG` can select a validated UTF-8 JSON policy of at most 64 KiB.
Existing `config/editorial.json` defaults remain unchanged. Optional
`language_formats` maps at most 20 lowercase language tags to complete format sets:

```json
"language_formats": {
  "fr-be": {
    "brief": {"min_words": 70, "max_words": 150},
    "standard": {"min_words": 350, "max_words": 600},
    "analysis": {"min_words": 800, "max_words": 1200}
  }
}
```

Resolve an exact lowercase tag, then its primary language, then default formats.
Each minimum is positive and no maximum is below it. The policy rejects unknown
keys and unsafe/unimplemented publication settings. File errors omit private paths.
Reading estimates use `reading_words_per_minute` (initially 200), rounded up; zero
words gives zero minutes in draft diagnostics, 201 words gives two minutes at the
default rate. A real publication always needs countable body content. Estimates
describe a configured reading assumption, not measured individual reading speed.

## Review and overrides

New revisions record an immutable assessment: count-policy version, words, language,
format, range, support status, policy fingerprint and optional public reason.
`RevisionInput.length_override` is optional draft data. When
`length_override_requires_reason` is true, out-of-range or unsupported-language
publication needs a nonempty reason (maximum 2,000 characters). A reason never
makes zero-word content publishable. If the setting is false, the ranges remain
visible editorial guidance and the ordinary publisher review still applies.

`EditorialService::length_revision` creates a candidate containing unchanged prose,
original provenance/sourcing and exact citation evidence versions. It replaces
the assessment, records the authenticated actor privately and resets approval.
Clearing a reason is a new candidate too. Classification/source-only edits retain
an existing assessment; source edits of old imports can record an incomplete
assessment but never invent an old exception. Prose revisions do not implicitly
inherit exceptions. A policy change invalidates a stale assessment for a new
publication; re-evaluate and review the candidate under the active policy.

The private draft page shows the count, current target range, estimated reading time
and explanatory warnings. Its length form requires live editor authority, exact
head version, CSRF and same-origin submission. The override reason is explicitly
public; the revision-change reason and actor stay private. Publication remains a
separate publisher action; this feature exposes no publication HTTP endpoint.

Public article pages show the count/estimate and retained public format choice.
Operator instructions and target-policy forms stay on private pages. Previously
published articles and imports remain readable after policy changes; they are not
rewritten or retrospectively assigned fabricated approval or override evidence.

## Implementation and verification

Use `EditorialService::with_policy` with the validated application policy for
configured editor/publishing paths. `new` deliberately uses bundled reference
defaults. The optional direct `unicode-normalization` dependency reuses locked
0.1.26 already in the tree; no new transitive package/version is introduced. Its
declared license is `(MIT OR Apache-2.0) AND Unicode-3.0`. This removes the need to
invent Unicode normalization tables; it is not a language segmentation engine.

Migration 17 appends only whitelisted public assessment fields. Public readers
retain their existing view grants; workers gain no authority. The ordinary editor
clone path uses existing restricted revision/citation/audit grants. No live grant,
schema, writer or environment change is implied by these source changes.

Run the normal Rust checks and `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`.
On small VFS Docker hosts, `WORLDNEWS_TEST_PG_TMPFS=1` uses capped disposable memory
storage and `WORLDNEWS_TEST_RECLAIM_BINARIES=1` removes each regenerable test executable
after its suite. Both preserve tests/assertions and do not replace the separate disk
persistence drill. See ADR 0019 and the evidence record for local profile constraints
and the complete integration result.
