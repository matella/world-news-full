# Private edition workbench evidence (#30 navigation/preparation slice)

Focused verification passed locally on 2026-11-17:

- Strict all-target Clippy and the opt-in restricted-writer `editions` integration.
- The existing disposable runner now exercises edition card/choice pagination and nonpersistent preparation in the same edition suite.

The test creates 25 private edition drafts and 25 synthetic current publications,
crosses keyset pages without repeated cards, inspects draft/all filters and retains
selected withdrawn/private identities across story-choice pages. A private story
has no invented publication title/revision; large stored bodies are not loaded for
choice metadata. Actual format/nature/language/classification come from the
published revision and shared taxonomy.

Own-input preparation keeps duplicate selections and ambiguous lead warnings,
shows stale/private selections and unknown publication instants, rejects a revision
that belongs to another story, and leaves edition membership/version/audit counts
unchanged. After identity revocation, card reads, story choices and preparation all
reject the old authenticated session. This does not claim a UI/browser composition
journey; the native screen remains #30 work.

Full regression and final build/runtime outcomes are recorded below before PR
publication. Parent schema-19 immutability/rollback/source-version tests remain in
the same suite. No new schema, grant, dependency, provider/source request, HTTP
endpoint, live deployment/data/account change or writer switch.

Self-review covers explicit current versus captured revision/version metadata,
bounded pages/retained choices, unknown publication fields, role/session lifetime,
revision ownership and nonpersistent preparation versus transactional save
authority. This is self-review, not independent review. Rollback removes these
consumers while retaining the compatible schema-19 binary and private history;
complete edition publication/removal policy is still #31.

The complete local runner passed all 21 disposable DB suites and eight existing
Chromium journeys, followed by final fmt, 61 unit tests, strict all-target Clippy,
build and process-runtime checks. Foundation/full GADS audit/deployment-policy/
secret/diff guards passed. Project-owned instructions reconcile #29's completed
manual workflow with the remaining edition gates; the 35 pinned managed files
are unchanged. GitHub full workspace/frontend/persistence checks must pass before
merge. No native edition composition browser journey is claimed in this slice.
