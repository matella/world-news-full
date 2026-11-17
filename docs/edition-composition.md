# Edition composition guidance

Related: #30, #31, #24; ADR 0028. The Rust preparation model and private draft service are implemented; the
protected selection interface and edition publication command remain pending.
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

## Private draft service

`editions::drafts::Drafts` creates an edition for the configured date/timezone
identity and saves an explicit sequence of current published revisions. New dates
must use years 0001–9999, matching canonical public routes. Every call
revalidates the live editorial session; saves require expected edition and story
versions. This initial service selects actual publications. A ready private article
must first be explicitly published through the separate article workflow; selecting
it never publishes it automatically.

Saving replaces membership and records an immutable private before/after journal
in one transaction. Duplicate entries, stale versions, withdrawn/private stories,
invalid section choices and timezone drift leave the old draft intact. A private
draft can be empty or lack a lead, with explicit preparation warnings. Published
editions cannot be edited. No public edition is created by these calls.

Reads preserve saved revision/order/fences and warn when a story has changed since
preparation. Imported memberships retain unknown preparation versions; the service
does not fabricate them. History pages contain at most 20 actions. The workbench
caps selection at 100 stories and aggregate JSON body text at 8 MiB before decoding;
large stored bodies fail explicitly without truncation. Existing public archive
reads are unchanged by these private bounds.

Migration 19 adds version/fence fields and private immutable history, without
rewriting existing memberships or public grants. Schema-18 binaries refuse it;
rollback needs a schema-19-compatible image or a separately verified restore plan.
See ADR 0029. No live migration or runtime grant is authorised by fixture setup.

Import events record import bookkeeping, not original publication instants. Only
explicit `published`/`corrected` revision events supply publication age; an
import-only revision remains unknown even when the import event has a recent
created_at. Do not substitute edition dates or revision creation times.

## Private workbench preparation

The service supplies 20-card edition pages (draft-only or all) ordered by edition
date and stable identity, with an exclusive older-page cursor. Story choices page
20 actual current publications by stable ID and retain up to 100 selected IDs,
including a now-private or withdrawn item. Missing publication metadata stays
explicitly absent; a private candidate title is not presented as a publication.
Choice format/nature/language and proposed section come from the actual published
revision and shared taxonomy, independently of language/geographic assumptions.

Choice metadata describes current stored state. It does not refresh a prepared
revision/version automatically. A nonpersistent preparation method retains own
selection order/lead and duplicate input, reports stale/private/withdrawn articles,
and rejects mismatched revision/story identities. It uses the same count/time/
byte bounds, without creating a draft, updating membership or writing edition
audit. Final save always rechecks actual publication pointers and all fences.

Every navigation/preparation call rechecks the live editorial identity/session.
These read models add no public route or additional writer privilege. The native
composition interface and its browser journey remain pending; see ADR 0030.
