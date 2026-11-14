# ADR 0007: Conservative URL identity and immutable intake observations

Status: accepted for staged implementation of #19, based on #18. Not deployed.

Feed metadata is input evidence, not a story or certified original reporting.
Version 1 uses a public HTTP(S) URL, removes only literal case-insensitive `utm_*`,
`fbclid`, `gclid`, `dclid` and `msclkid` query segments, and retains other query
segments in their original order/encoding, duplicates and fragments. URL parsing
also applies standard URL serialization (hostname case/default ports/dot segments).
Percent-encoded tracking keys remain untouched. No redirect probing occurs here;
the article has not been fetched. Redirect aliases require reviewed evidence in
future extraction, rather than automatically changing immutable source identity.

The key is `url-v1:` plus SHA-256 of that URL. Article IDs use the same digest.
A batch transaction serializes with other normalizer transactions using a shared
Postgres advisory lock. Unique canonical keys and immutable source identity remain
DB constraints. Match by key or exact original/canonical URL to recognize legacy
records. Multiple identities or a different configured source quarantine rather
than overwrite or assign the wrong attribution. Unknown historical tracking
variants require archive reconciliation; this does not retroactively repair them.
The archive importer must run during the planned write freeze, not concurrently
as an uncontrolled second article writer.

Headlines, parser IDs and similarity are never identity keys. Distinct outlet URLs
remain distinct even when syndicated; registry origin links remain hints and no
article origin is invented. Changed semantic metadata creates a review observation
without rewriting the first article. A fingerprint includes canonical URL, trimmed
title, parser dates, normalized language and author names. It excludes parser ID
and tracking variants. Differences are metadata changes, not proof of body changes.

Every entry receives an immutable private observation and each batch an immutable
receipt in the same transaction as inserted metadata-only articles. Disabled
configuration holds a batch with no receipt; changed configuration quarantines its
entries. Replays return the original receipt. Existing stories, extracted bodies,
article identities and public citations are never modified by this service.

Tradeoffs: conservative matching can retain duplicates; ambiguous links need
review. One advisory lock limits initial throughput but keeps concurrency simple.
No new dependency, scheduler, article network fetch or public publishing endpoint.
Future identity versions must preserve original evidence and define migration.
