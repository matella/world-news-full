# ADR 0015: Stable primary subject threads and reversible curation

Status: accepted for #25 core services, based on #69. Staged, not deployed.

A thread is a persistent subject identity; each story has at most one primary
thread. Multi-topic/geographic relevance remains taxonomy, not duplicate subject
membership. Explicit dated stories are updates within a thread. Similar titles and
legacy related-ID graphs do not establish identity or authorize a merge.

Reuse owned threads/thread_stories. Migration 14 adds optimistic versions, merge
aliases, optional explicit update instants, unique primary membership, bounded audit
snapshots and restricted public read models. Refuse ambiguous/oversized old
membership instead of deleting or guessing. Existing added_at is bookkeeping and
is never backfilled into an asserted update time.

All curation requires a live publisher session, expected versions and reason.
One transaction advisory lock serializes this small graph; short lock/statement
budgets bound contention. Per-thread membership is capped at 200, mutations affect
at most two threads, and immutable before/after snapshots record the actor and
inverse relationship. Move assignments, versions and audit atomically.

Merge active canonical subjects into a canonical destination, preserving the old
ID as an alias. Split an explicit subset into a new stable ID. Undo appends an
inverse event only when affected states/versions still match the recorded result;
subsequent edits cause conflict rather than rewinding someone else's work. Close
created identities when undoing creation/split, never delete identity/history.
Inverse-of-inverse permits an explicit redo. Alias depth is capped at twenty edges;
active canonical merge checks and exact-state undo prevent service-created cycles.

Public views expose only currently published members, one deterministic latest
public head per canonical thread, and distinct published edition/date counts.
Withdrawn/private future members cannot become heads or inflate public counts.
Unknown times sort last; ties use stable IDs. Topic/geography/language filters apply
together to one public update and use the taxonomy policy's canonical/alias keys.
The resulting latest head may differ from the update matching the filters.

Alias resolution uses a fixed-search-path, bounded SECURITY DEFINER SQL function
that returns an ID only when the final subject has a public head. Revoke PUBLIC
execution; grant only reviewed reader roles. A public timeline resolves and reads
members in one repeatable read-only transaction to survive concurrent merges.
Keyset pages use time/ID with explicit NULL ordering, not offsets or title similarity.

The APIs support the future editor/reader/search work. This core service PR does
not switch the transitional related-ID UI or claim #29/#38/#39 implemented. No
HTTP publication, subscription, live backfill/grant or deployment is introduced.
