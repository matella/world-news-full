# ADR 0017: freeze article-specific reporting-origin evidence

Status: accepted for implementation. Related: #19, #27; follows ADRs 0005 and 0016.

An outlet's registry relationships are configuration hints. Two outlets carrying
the same dispatch do not establish two independent reporting origins. A legacy
origin link or claim identifier does not establish article-level evidence either.

Use publisher-authored, append-only assertions on individual source articles.
Each assertion records unknown/documented/conflicting state, a public basis,
private evidence note, actor, source version and captured origin labels. Seal the
labels atomically with the monotonic article pointer. Keep intake and origin
versions separate. Publisher edits and concurrent assertions use live session,
role and expected-version checks; no worker impersonates a human curator.

Citations freeze the selected sealed evidence version and optional validated body
anchor. Existing rows upgrade to explicitly unknown evidence and no guessed
anchor. Classification-only revisions copy the exact citation evidence versions;
they cannot silently refresh attribution or alter published snapshots. Public
views expose frozen labels/basis and currentness, never private notes or actors.

This models attribution, not truth. It does not infer independent reporting from
outlet count, certify a claim, or implement the complete publication policy. Claim
binding/editing, attributed disagreements, count presentation and publication gates
remain #27. No publishing HTTP endpoint becomes available through this change.

Migration 15 is additive except for transactional population of its new citation
columns. Do not roll back by deleting assertions, labels or published evidence.
After new evidence writes, use a compatible reader/writer image and a forward fix;
live migration and grants require the concrete release plan in #47–#49.
