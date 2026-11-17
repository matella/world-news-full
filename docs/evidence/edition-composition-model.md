# Edition composition model evidence (#30 preparation slice)

Passed locally on 2026-11-17:

- `cargo test -p world-news --locked --lib`: 61 passed, including six new composition tests and the strengthened startup-policy test.
- `cargo fmt --all -- --check` and `cargo clippy -p world-news --all-targets --locked -- -D warnings`.
- `cargo build -p world-news --locked` and `python3 scripts/test_rust_runtime.py`.
- Foundation/GADS managed-file check, full pinned GADS repository audit (35 passed, zero failed/warnings), deployment-policy/secret-pattern guards and `git diff --check`.

Tests exercise deliberate order and non-first lead, aggregate word counting/rounding
without prose mutation, actual format/nature, duplicate/ambiguous lead preparation,
missing sections, over-budget concentration, separate stale/future/unknown times,
withheld whole-edition minutes for approximate languages, no language/geography
inference, empty/oversized/invalid selections, configurable thresholds and unchanged
article-length fingerprints. Older policy files still parse with bounded defaults;
unknown required sections fail startup with a redacted error. The observation
instant is explicit, not a hidden host-clock assumption.

This is a pure read/preparation model and configuration contract. No database or
browser interface is added. The 20-DB/eight-Chromium local run belongs to the parent
article-workflow source and is not claimed as rerun on this slice. GitHub must run
its full workspace/database/frontend/persistence checks before merge. No SQL,
dependency, provider, live data, deployment, accounts/grants or writer/traffic change.

Self-review covered bounded input, deterministic preserved order, ambiguous lead,
unknown times, supported versus approximate counts and the separation between
warnings and service authority. This is not independent review. Duplicate/private/
stale publication eligibility must be enforced by the future live-session/versioned
service; the model provides no ready/publish operation. Draft persistence, protected
selection interface and snapshot publication remain #30/#31, which stay open.

Rollback reverts source and restores the matching prior policy file for an older
binary that rejects the new `edition` key. Article-length fingerprints and stored
assessments are unchanged by edition guidance. No migration is required.
