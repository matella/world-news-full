# ADR 0028: explicit edition selection with transparent composition observations

Status: accepted for implementation. Related: #30, #31 and #24.

## Decision

Prepare edition composition as a bounded pure Rust projection before wiring the
versioned draft/publication commands and protected interface. It accepts up to
100 immutable revision selections, an explicit lead story, configured section IDs
and a supplied observation instant. Input order is preserved, including duplicate
unsaved input so it can be corrected. It never ranks by outlet/reporting-origin
count or changes article prose, sourcing, approval or publication state.

Add an optional `edition` object to the existing validated editorial policy:
minimum/maximum story targets, publication-age threshold, section-concentration
threshold and required section IDs. The repository chooses 3–12 stories, 48 hours,
60 percent and Belgium/international section warnings. These are review guidance,
not geopolitical judgements or automatic publication gates. Old policy files omit
the object and receive documented defaults with no required sections. Reject
malformed/bounded-policy values and unknown required sections at app startup.
Edition guidance stays out of the article-length fingerprint: changing composition
targets does not invalidate otherwise unchanged article-length assessments.

Compute word-v1 body counts without overview/reference/interface text and round
once over the combined words. If any selected article has an approximate-language
count, withhold the numeric whole-edition estimate; do not present a supported
subset as the whole edition. Format/nature labels come from the actual revision.
Sourcing labels/counts remain the citation projection's responsibility.

Warn explicitly about empty/partial composition, story-count/reading targets,
duplicate story/revision identities, absent/ambiguous lead, missing required
sections, incomplete/unmapped classification, section concentration and old/future/
unknown publication instants. A publication instant measures publication age,
not freshness of the facts. A draft/unknown instant stays unknown; never substitute
revision creation, feed/request or host time. Language never implies geography.

## Consumers and boundaries

The projection deliberately does not certify eligibility or readiness. The future
service must load stored identities/revisions/times, reauthorise live sessions,
fence edition/story versions, reject duplicates and invalid/empty publication,
require explicit publication decisions and preserve edition snapshots. This slice
has no database mutation, editor screen, article/edition publishing endpoint or
worker/provider call. #30 remains open until those consumers and complete journeys
pass. #31 owns complete transactional snapshot publication and removal policy.

No schema/dependency change. Configuration rollback requires the matching older
configuration when using a binary that does not recognise `edition`. Source merging
does not deploy either configuration or binary. See docs/edition-composition.md.
