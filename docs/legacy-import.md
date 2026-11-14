# Legacy published-archive import

Issue #12. This is an explicit historical-data import, not publication of new
intake or generated text. It never contacts Jarvis or discovers a live database.
New writing still needs the future editorial review/publish workflow.

## Input and privacy

Use an authorised JSON export shaped as an object with a `stories` array and an
optional `corrections` array. Each story uses the legacy fields in the baseline:
id, title, body, lang, edition_date (YYYY-MM-DD), updated_at (RFC3339), optional
title_fr/body_fr, topic, synthesis/count flags, source links, claims/disagreements
and related_ids. Sources use sources_json (or the explicit sources alias). Conflicting
aliases are quarantined. Corrections identify story_id; no missing old revision
body or timestamp is invented from a correction note.

An authorised operator can export `jsonb_build_object('stories',jsonb_agg(to_jsonb(p)))`
from the actual published_stories table after verifying schema/access. This is a
format example, not a query this agent ran against production. Do not export drafts
as already-published history. Keep source exports and reports in access-controlled
storage outside Git (work/ is ignored). Never paste article/credential data in issues.

Limits: 32 MiB per input, 10,000 rows, 200 entries per complete edition, 200 source
links/1,000 related hints per row and 1 MiB body text. Split large exports by complete
editions. Overflow/invalid data is reported or quarantined, never truncated. JSON
number precision is retained; unsupported NULs, numbers over 512 lexical characters
or exponents outside +/-308 are quarantined before Postgres JSONB insertion.

## Plan first, apply explicitly

```sh
# Build first. Neither dry-run form needs DATABASE_URL or network access.
cargo build -p world-news --locked
target/debug/world-news --import-legacy work/archive.json --dry-run
target/debug/world-news --import-legacy work/archive.json

# Against an authorised, migrated STAGING target; credentials injected separately:
target/debug/world-news --migrate
target/debug/world-news --import-legacy work/archive.json --apply
```

The import timezone comes from NEWS_EDITORIAL_CONFIG or the compiled editorial
default (Europe/Brussels). Dates are preserved as reported, not derived from host
timezone or shifted from updated_at. Applying uses the validated application DB
configuration and requires the complete matching migration set plus write privileges.

Stdout is JSON metadata: row IDs, counts, hashes, warnings, quarantine codes and
ID/revision mappings. It excludes article text, titles, source URLs and raw input.
Logs go to stderr and contain only safe operation/error codes. Reports still carry
archive identifiers and should remain private; hashes are not anonymisation.

Exit codes: 0 means the operation succeeded without quarantine; 2 means a report
requires attention because records were quarantined (valid rows may have committed
under --apply); 1 means a hard failure. Warnings are not hidden and must be reviewed
before release. Dry runs do not check destination conflicts because they are offline.

## Mapping and integrity

- Preserve valid legacy story IDs, so existing /story/:id URLs need no redirects.
  Revision IDs are deterministic SHA-256 fingerprints of canonical JSON keys and
  matched correction notes. Object key order/pretty-printing does not change them.
- Persist original semantic row JSON and matching correction notes in immutable
  legacy_records. Retain the original file for exact bytes and quarantined rows.
- Preserve original title/body/language, French display variants, reported counts,
  synthesis flags and relationship hints. Paragraphs stay plain text; do not infer
  headings, rewrite text, pad length or silently translate.
- Last known legacy updated_at is retained and labelled as its timestamp semantics;
  it is not certified original publication time. Import events/edition published_at
  record the new local import action, while edition_date retains the archive day.
- Legacy summary/synthesis and translation review state are explicitly unverified.
  French display overrides are kept even for already-French originals. Partial
  variants are warned about, never filled with invented translations.
- Source identity is conservatively inferred from hostname for legacy references;
  original labels/links remain in the payload. Source articles store metadata and
  links only, not fetched outlet bodies. No reporting origin is invented from a count.
- Article-level references are not claim verification. Claim index semantics are
  unknown: claims and original disagreements stay in original metadata with warnings
  for later mapping. No zero/one-based source attribution is guessed.
- Unresolved related IDs stay as hints and warnings; no irreversible thread merge
  occurs. Correction notes do not fabricate previous content/history.

Whole valid-row batches commit atomically with stories, references, translations,
ledger and complete edition snapshots. An advisory transaction lock serialises
imports. Unexpected failure rolls back the batch; exact rerun is safe even if a
previous client lost its commit acknowledgement. DB tests inject a mid-batch error.

Exact previously imported hashes skip without touching later manual edits. Changed
IDs, existing manual stories and occupied edition dates are quarantined. The tool
does not silently overwrite reviewed work or mutate a published snapshot. Duplicate
identical rows are counted/deduplicated; conflicting duplicate IDs are quarantined.

## Staging and final handoff

1. Import into a fresh staging target, review quarantine/warnings and verify counts,
   identifiers, source links, translations and representative archive pages.
2. Keep the external writer and old reader untouched while staging is tested. Retain
   the source export/report and target backup under the operational retention policy.
3. If the legacy export changes, changed records are conflicts, not automatic updates.
   Rehearse a fresh full import into another staging DB, split by complete editions.
4. For final cutover, obtain explicit writer-freeze/deployment authorisation, take a
   final complete export, and build/verify a fresh target from that frozen snapshot.
   Compare it with the earlier report and preserve any independently curated new-app
   content through an explicit reconciliation plan. Never drop an existing target
   merely to resolve conflicts. Keep the old stack/database for rollback.
5. Only #47/#49 recovery/cutover gates can approve changing reader traffic. Import
   success alone is not a release or proof of editorial correctness.

Fresh-target reimport is the safe final-delta strategy in v1. In-place merge/update
of changed archives is deliberately unsupported; published snapshot immutability
and manual work must be preserved. No automatic deletion/undo command is shipped.
