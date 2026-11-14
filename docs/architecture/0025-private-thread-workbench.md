# ADR 0025: private thread preparation and explicitly unknown instants

Status: accepted for implementation. Related: #29; extends ADRs 0015 and 0024.

Prepare the human curation interface with bounded live-session private thread cards,
story choices and inverse-action previews. Cards include private members and aliases;
they are not public reader counts. Choice pages retain explicit selections and show
current revision metadata without substituting published prose or inventing a title
for a draft with no current revision. Such incomplete rows remain explicitly marked
by absent revision/title metadata; they are never successful empty article previews.

Inverse previews read owned immutable before/after snapshots and compare recorded
affected states with current states. Eligibility is a read-time observation, never
restore authority. The existing undo transaction still locks and checks every exact
state/version and appends an inverse action. Do not accept caller snapshots or copy
old JSON over newer memberships. Private actor/reason/member data has no public DTO.

The original assign API required a known timestamp even though storage/snapshots
support unknown legacy instants. A curator must be able to classify an archive
without fabricating a date. MemberInput.update_at is now Option<DateTime<Utc>>:
Some records an explicitly known UTC instant at database microsecond precision;
None explicitly records unknown. Existing Rust callers wrap known instants in Some.
There was no exposed HTTP assign contract to migrate. A future form must require
an explicit known/unknown choice rather than treating an omitted field as evidence.

Changing known/unknown status retains bookkeeping added_at, increments graph version
and records before/after history. Never substitute added_at, request time, feed time
or article publication time for a missing development instant. Undo/redo preserves
unknown values; versions remain monotonic even when semantic membership is restored.

No schema, dependency, source HTTP, curation/publication HTTP endpoint or production
change. Old readers already support nullable instants and snapshots. Rollback keeps
all graph/history data and must never invent timestamps for compatibility. Full
curation forms and the complete #29 publication journey remain separate acceptance.
