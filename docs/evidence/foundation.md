# Foundation evidence

Scope: #2, #3, #4, #9. Risk R1 for docs/fixtures, R2 for agent/CI configuration.
Source baseline: d67dc5948efd89615706c571e4f26090fac82b71.

## Passed locally

- Upstream `scripts/verify_manifest.py`: PASS before installation.
- Installer `--dry-run`: 35 managed files plus project seed, no conflicts.
- Installer without force: 35 managed files installed and hash lock recorded.
- `python3 -m unittest discover -s ../gads-source/tests -v`: 73 tests passed.
- `python3 ../gads-source/scripts/repo_audit.py .`: 35 passed, 0 failed, 0 warnings.
- `python3 scripts/check_foundation.py`: all managed hashes/settings checked;
  synthetic brief/standard/analysis examples contain 115/377/932 whitespace words;
  4 synthetic legacy rows with null/overlapping/missing-relation cases validated.
- `git diff --check`: passed.

The first fixture-generation attempt failed on an intentional null edition date;
the generator was corrected before fixture validation passed. No application
behaviour or production data changed.

## Inspection and limits

Reviewed source routes, queries, runtime settings, manifests, workflows and guard
hook; compared current-state inventory with their source files. Reviewed managed
file diff and kept local rules outside GADS-managed paths. Documentation describes
current bugs rather than claiming they have been fixed. Synthetic fixture domains
use `.invalid`; article examples are expressly fictional.

Upstream docs specify Python 3.15; local evidence is Python 3.12.14 only. Claude
runtime test: not run (CLI absent). Codex session read policies and project rules;
isolated CLI instruction-discovery test not run. Neither is certified compatible
by static checks. Application build/browser/database tests are not applicable to
this docs/policy/fixture batch; they remain required for code-changing batches.

## Rollback and remaining gates

Revert the foundation commit to remove this adoption; no schema or runtime change
needs reversal. #3's Claude runtime smoke gate remains unverified. CI workflow is
prepared but a remote result must be reported separately. No merge/deployment,
external publisher call, live schema read or production backup has occurred.
