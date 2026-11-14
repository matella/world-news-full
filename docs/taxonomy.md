# Editorial classification and sections (#24)

The canonical policy is config/taxonomy.json. Article language describes the text;
topics describe subject matter; geography describes editorial relevance. An article
may have multiple topics/regions, or none when unknown. French/Dutch/Spanish never
implies a country. Source homepage/coverage metadata is not article classification.

Defaults include politics, economy, technology, science, health, environment,
society, culture and sport, with Belgian, European and international relevance
choices. These are initial configurable choices, not a claim that every region or
topic has been classified. Add reviewed canonical tags/sections when needed.

Each section rule contains any_topics and any_geography. A nonempty dimension
requires a match; if both dimensions are nonempty, both must match. The first rule
in the array wins. Default priority is sport, sciences/tech/economy, Belgium, Europe,
international, then Non classé. Priority changes presentation, not stored multi-tag
membership or independent origin counts. Language cannot be a selection rule.
The future edition/search implementations must consume taxonomy.rs and this policy,
not reproduce language shortcuts. Those pages remain their own roadmap work.

Rust embeds the repository policy by default. NEWS_TAXONOMY_CONFIG may select a
UTF-8 JSON file up to 64 KiB; startup/import validation rejects malformed/unsupported
settings without printing paths/content. Definitions are bounded and canonical IDs
are explicit lowercase keys. Classifications allow up to twenty distinct IDs per
dimension. Editor writes require canonical IDs; explicit aliases exist only for
historical read/mapping projections. Config hashes include rule order.

The transitional Next build uses frontend/src/lib/taxonomy.generated.json. After
changing canonical policy, copy its exact bytes to that generated file and run
python3 scripts/check_foundation.py. This preserves the existing frontend-only
Docker context. Next has no independent runtime file override; align the reviewed
Rust policy with its build snapshot while both applications are in use. No live
image/config/deployment is changed by a policy source edit.

## Editor overrides

The private draft page provides topic/geography checkboxes and a required reason.
An authenticated editor may replace the classification of the expected candidate
version; unknown/duplicate fields or tags are rejected and stale versions conflict.
The POST creates a new revision, copies prose/language/kind/citations and original
provenance, records actor/reason plus classification policy hash/snapshot, resets
review and leaves public revision/edition snapshots untouched. A failure copying
citations rolls back the complete revision/head/audit transaction.

Old unmapped labels are called out. The editor explicitly chooses the replacement
set; the original remains in immutable history. No writing attribution or historical
pipeline marker is replaced. Translation/review records stay on their original
revision; complete language-variant status remains #33. Classification alone never
publishes, grants extraction or fetches a source.

The editor-runtime identity additionally needs reviewed SELECT/INSERT on citations
and SELECT on publication_events, alongside the existing editorial grants, so the
published-citation guard can run. Disposable fixtures test those exact operations;
this is not a live provisioning script. Do not widen public-reader authority.

## Legacy mapping and backfill review

The existing offline --import-legacy FILE --dry-run report now includes
 taxonomy_policy_hash and each validated row's language/proposed_classification.
The proposal contains canonical topic IDs, empty/unknown geography, a primary
section and unmapped flags. It does not expose unknown raw labels or guess a country
from language, title or source. Current/old imports and immutable payloads retain
their original data. The apply report repeats the same proposal for inserted or
already-imported rows; it does not apply it silently. Input hashes/ledger identities
remain based on original records, so changing policy cannot overwrite an old import.

Review proposals and unknowns before planning backfill. Use authorized editor
revision overrides for decisions; a live bulk backfill needs the actual archive,
reviewed mapping/reconciliation report and release authorization. Existing frozen
edition section labels are historical snapshots and are not recomputed by this
source change. Store the reviewed config with release/import artifacts.

Examples tested: French global science -> sciences; Dutch Belgian politics ->
Belgium only with explicit be; Spanish world society -> international only with
explicit world; science/sport across regions -> configured sport priority; politics
with unknown geography -> unclassified. A bare legacy Monde label remains unmapped
rather than becoming a country or a fabricated reporting topic.

Rollback serves the previous application/policy while retaining revision/audit data;
old data is not deleted to revert section rules. A label/config rollback cannot
revoke a published correction or replace release reconciliation.
