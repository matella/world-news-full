# Manual editorial workflow evidence (#29)

Focused local verification passed on 2026-11-17:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `cargo clippy -p world-news --all-targets --locked -- -D warnings`.
- Disposable restricted-writer `cargo test -p world-news --locked --test workflow -- --ignored --test-threads=1`: passed.
- `scripts/test_workflow.py` Chromium journey against that same disposable fixture: passed.

The HTTP journey covers editor/publisher boundaries, exact revision/version fences,
unknown forged-ready fields, CSRF/Origin rejection, review changes with private
notes, source reconfirmation and review reset after prose edits, required public
reasons, correction publication while preserving an edition’s old revision, a
withdrawal with a newer pending candidate, historical/private read boundaries,
revocation/demotion and a draft-only automated-summary kind. It upgrades migrations
1–17 to 18 with an existing narrow reader grant and published event: the public
initial reason becomes readable, private actor/base-event access stays denied.
There is no actual AI provider call or generated-content integration claim.

The browser uses separate author, publisher and anonymous sessions. It completes
original authorship, explicit human-method proof, classification, submission,
approval, keyboard publication, correction/reconfirmation/review/publication and
withdrawal. It checks anonymous public reasons, escaped markup, no private proof/
review notes, no article after HTTP 410, native controls and narrow mobile layout.
External requests are intercepted and none occur. The synthetic screenshot remains
a local review artifact, not production evidence.

The first full regression attempt failed during linking the HTTP test executable:
the small VFS/Docker host exhausted disk headroom. Its disposable container was
removed in finally. Obsolete task-owned test binaries were reclaimed and HTTP was
prelinked before the rerun; no host prune, assertions or source data were changed.
Full regression and final-check outcomes are recorded below before PR publication.

Self-review covers live service authority versus observed readiness, immutable
candidate/published pointers, exact stale fences, private/public projections,
withdrawal of the actual publication and schema-18 rollback compatibility. This is
self-review, not independent review. No live migration, account/grant change,
source network, provider cost, deployment or writer/traffic switch occurred.

Migration 18 retains the existing public-notice view shape/grants and adds initial
publication events. No rows are rewritten. Schema-17 binaries refuse schema 18;
rollback requires a compatible image and an explicit release plan. No dependency
was added. GADS-managed files are unchanged; project-owned instructions document
the new contract. Unchanged frontend checks are delegated to GitHub CI before merge.

## Full local regression

`WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`
passed all 20 disposable database suites and eight Chromium journeys on the final
Rust source. Existing publication/state assertions were preserved; withdrawal URL
expectations deliberately change from 404 to the new documented 410 notice page,
with stronger reason/no-article/private-data assertions. No test was disabled.

`python3 scripts/check_foundation.py` passed all 35 pinned GADS files;
`python3 ../gads-source/scripts/repo_audit.py .` passed 35 checks, zero failures or
warnings. Deployment-policy/secret-pattern guards and `git diff --check` passed.
These checks do not prove live agent enforcement or absence of every possible
secret. Final Rust build/unit/runtime results follow before publication.

Final `cargo fmt --all -- --check`, `cargo clippy -p world-news --all-targets --locked -- -D warnings`,
`cargo test -p world-news --locked --lib` (55 passed),
`cargo build -p world-news --locked` and `python3 scripts/test_rust_runtime.py` all
passed after the full database/browser regression. Staged foundation/secret guards
and diff checks passed. GitHub must pass its complete workspace/frontend and
persistence workflows before merging the tested source SHA.
