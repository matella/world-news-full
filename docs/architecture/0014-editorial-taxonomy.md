# ADR 0014: Explicit editorial dimensions and versioned section rules

Status: accepted for #24, based on #68. Staged, not deployed.

Keep article language, topics and geographic relevance independent. Reuse revision
language/topics/geography fields; no schema rewrite is needed. Empty topic/geography
sets are explicit unknown/unclassified states, not evidence of world/Belgian coverage.
No source language, publisher address, source-registry hints or ML guess certifies
an article's geography.

Use config/taxonomy.json as a versioned policy with canonical tags, explicit legacy
aliases, French labels, ordered section rules and a fallback. Rust accepts a bounded
NEWS_TAXONOMY_CONFIG override. Reject unknown schema keys, duplicate/ambiguous tags,
bad references, empty rules and oversized settings. A rule matches any configured
topic and any configured geography, requiring both dimensions when both are specified.
The first matching rule wins; language cannot appear in a rule. Preserve all topic/
geography values even when an edition selects a single primary section.

The shared Rust projection supports admin and subsequent edition/search consumers.
Legacy aliases may project to known topic IDs, but a legacy world/country label is
not silently converted into geographic relevance. Import reports include the policy
fingerprint, language and proposed canonical classification/unknown flags. Preserve
original imported fields/payloads, ledger identity and frozen editions; this report
is a conservative backfill proposal, not an automatic historical rewrite.

An editor override creates a new revision under the live session, expected story
version and reason. Copy content, language, kind, citations and original provenance;
add the classification basis, previous revision, policy hash/snapshot and selected
section. Reset review; retain the published revision/snapshots. A classifier/editor
is not retroactively the author of legacy content. Translation/review records remain
tied to their original revision and are not silently transferred as approval.

The transitional Next frontend cannot import a file outside its existing Docker
build context. Use an explicitly generated copy in frontend/src/lib, with exact
byte drift enforcement in check_foundation.py, rather than changing deploy contexts
or maintaining a separate policy. It uses the same rules for grouping/kickers, has
no language-to-Belgium fallback and retains all bounded story-list entries. Its
build uses repository policy; runtime overrides apply to Rust. Keep transitional
configuration aligned during coexistence; retire the generated copy with Next #49.
