# GADS adoption and runtime evidence

Upstream: [GADS 1.3.0 at ad6f6fb](https://github.com/matella/global-ai-development-standard/tree/ad6f6fb5e6bb2afd2b1168df64510f77f61b8ea7).
Related issue: #3. License: Apache-2.0, retained in `gads/LICENSE`.

The upstream manifest was verified before install. The reviewed installer was
previewed with `--dry-run`, then run without `--force`; no existing instruction
files were overwritten. `gads/install.json` is the upstream-managed hash lock;
`gads/UPSTREAM.json` records the immutable source revision. Project rules live in
`gads/PROJECT.md`; keep managed policies/roles/hooks unchanged for upgrades.

## Reproduce source validation

```sh
git clone https://github.com/matella/global-ai-development-standard.git ../gads-source
git -C ../gads-source checkout --detach ad6f6fb5e6bb2afd2b1168df64510f77f61b8ea7
python3 ../gads-source/scripts/verify_manifest.py
python3 -m unittest discover -s ../gads-source/tests -v
python3 ../gads-source/scripts/repo_audit.py .
python3 scripts/check_foundation.py
```

For upgrades, inspect the new revision and manifest, use installer `--upgrade
--dry-run`, review differences and merge project changes. Update the pin, CI ref
and provenance together. Do not silently upgrade from main.

## Runtime compatibility and limits

Upstream installation documentation requires Python 3.15. This environment has
Python 3.12.14; inspected scripts have no minimum-version gate and all 73 upstream
tests passed with 3.12.14. CI records this locally tested compatibility; this is
not a claim that upstream supports 3.12. Managed files remain unchanged.

Codex: repository AGENTS.md directs the agent to explicitly read gads/PROJECT.md
and canonical policies. This Codex session read both and executed the scoped
foundation task. This is a task smoke exercise, not an independent certification
of discovery precedence or sandbox enforcement. A new isolated CLI discovery
test can be recorded when its configured runtime is available.

Claude Code: CLAUDE.md imports AGENTS.md and project rules; rendered roles and
PreToolUse hook are installed. Upstream tests exercise launcher/guard handling,
including fail-closed behaviour. The Claude CLI is absent here; live instruction
discovery and hook invocation are **not run**. Repeat a harmless local task and
guard validation in Claude before claiming runtime compatibility. No live agents
are spawned by installing role definitions.

Claude deny rules/hooks do not apply to Codex. Pattern guards are not a security
boundary; runtime sandboxing, credentials and GitHub controls remain separate.
The audit checks structure and hashes, not evidence authenticity or release safety.
