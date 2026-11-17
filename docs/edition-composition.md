# Edition composition guidance

Related: #30, #31, #24; ADR 0028. The Rust preparation model is implemented; the
protected edition draft/selection interface and publication service remain pending.
This page describes observations, not an available publication command.

An edition uses an explicit sequence of selected article revisions and one chosen
lead story. The model retains that sequence; source/origin counts never choose a
lead. Choose sections from the configured taxonomy, independently of language.
Actual revision format/nature remains available for content-driven labels; sourcing
counts must come from the existing citation report, never a guessed score.

`config/editorial.json` contains:

| Setting | Repository value | Meaning |
| --- | --- | --- |
| `edition_target_minutes` | 10 | Whole-edition reading warning target |
| `edition.min_stories` / `max_stories` | 3 / 12 | Story-count warning range, not a filler quota |
| `edition.stale_after_hours` | 48 | Age since known publication, not factual freshness |
| `edition.max_section_share_percent` | 60 | Warn above this share once the minimum story target is reached |
| `edition.required_sections` | `belgium`, `international` | Configurable missing-section warnings |

The optional `edition` object uses defaults 3/12/48/60 and an empty required-section
list when absent, preserving older policy-file parsing. Its fields are explicit
when supplied; unknown/duplicate section IDs, invalid ranges and unknown keys fail
validation. Story maximum cannot exceed the hard 100-selection preparation bound.
Required sections must exist in the active taxonomy at application startup.
Composition settings do not change article-length assessment fingerprints.

Reading time sums body word-v1 counts, then rounds up once using the configured
words/minute. Overview/source/interface text does not count. If any language has
only an approximate count, show that limitation and omit the numeric whole-edition
estimate. Never shorten prose, omit caveats or add filler to meet a target.

Empty editions remain a preparation warning, with no claimed publication. Partial
editions show missing sections and count warnings; a later publisher interface
must make the publication decision explicit. Duplicate selections and a missing/
ambiguous lead remain visible in unsaved preparation input; they must be rejected
by the eventual transactional publication service. No automatic deduplication or
reordering hides the editor's input.

Dates come from stored publication events. Unknown/private candidates have no
invented publication time. Future dates and old publications are separate warnings;
none claims how recently facts were checked. Section-share counts are transparent
concentration observations, without an automatic geopolitical balance judgement.

Anonymous reading, existing imported editions, article publication and edition
snapshots are unchanged by this model. There is no schema migration, source HTTP,
AI call, scheduling or live release. To roll back to an older binary, restore its
matching policy file because it rejects the new unknown `edition` key.
