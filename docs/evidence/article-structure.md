# Structured article evidence (#26)

Based on #70; staged only. No migration, dependency, live content/grant/account,
provider, publication endpoint, deployment or writer action is introduced. Plain
v1 paragraph/heading data remains unchanged; v2 writes require compatible images.

Focused checks passed: strict Clippy/build and the new isolated persistence/render
suite, followed by real Chromium anonymous reading of brief/standard/long formats,
keyboard summary expansion, stable anchors, explicit context/uncertainty sections,
inert HTML-looking prose, unchanged legacy mapping and private draft 404. The
structured screenshot was inspected. A fixture initially lacked the existing
schema-version SELECT grant; it was corrected without widening application roles.

Final checks passed:

- `cargo test -p world-news --locked`: 43 unit/in-process tests.
- Strict fmt/Clippy/build and `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`:
  all sixteen isolated DB suites plus final CLI/HTTP/auth/readiness/shutdown and
  all three Chromium workflows passed.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`,
  `python3 scripts/check_secrets.py`, `git diff --check`: passed. The secret guard
  is a narrow tracked-file pattern check, not a complete security/history audit.

Coverage includes stored-field/body round trips in original language, separate
format/kind labels, duplicate presentation suppression without mutating strings,
body_schema provenance, exact claim anchor IDs and source markup remaining visible
plain text. Tests reject arbitrary HTML/style shapes, recursive/empty/duplicate
sections, duplicate/unsafe IDs and excessive nested text/block counts. Heading-only
content cannot enter review. Legacy literal Markdown/HTML does not become new
headings, summaries, analysis or uncertainty; no metadata is manufactured.

The shared Askama partial renders public and private draft bodies. Preceding admin
browser regressions exercise the private path; the new anonymous browser verifies
native details keyboard behavior and no external requests. No frontend source changed;
its unchanged base has passed CI and is not rebuilt for this Rust-only slice.

Self-review is not independent human review or full reader/accessibility acceptance.
Citation binding/publication integrity remains #27, language-aware counting/overrides
#28, editor #29 and reader design/provenance #34–#36. A structurally rendered article
is not certified sourced, within budget or ready for a real edition. Live release and
rich-body rollback compatibility remain under #47–#49; old-only readers cannot be
assumed safe after new variants are written.
