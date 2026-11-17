# Private thread-curation evidence (#29 interface slice)

Passed locally on 2026-11-17:

- `cargo test -p world-news --locked --lib`: 55 passed.
- `WORLDNEWS_TEST_PG_TMPFS=1 WORLDNEWS_TEST_RECLAIM_BINARIES=1 WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py`: all 19 disposable database suites and seven Chromium flows passed.
- `cargo fmt --all -- --check`, `cargo clippy -p world-news --all-targets --locked -- -D warnings` and `cargo build -p world-news --locked`.
- `python3 scripts/test_rust_runtime.py`, `python3 scripts/check_foundation.py`, `python3 scripts/check_deployment_policy.py`, `python3 scripts/check_secrets.py` and `git diff --check`.

Real restricted-writer HTTP exercises create/edit, nonpersistent story lookup,
explicit known/unknown date handling, member removal, selected subset split,
exact split inverse, two-stage merge, both-version fences, preserved aliases and
stale input recovery. A later destination title edit blocks an older merge inverse
without overwriting it; the submitted private reason remains recoverable. Actor/
snapshot spoofing, anonymous/role/CSRF/Origin failures and private-only public
resolution are rejected. No graph table is written directly by the controller.

The new Chromium journey covers the same native forms with keyboard saves, lookup
without mutation, null/offset dates, split/inverse, merge confirmation/alias/inverse,
removal, closure, inert title markup, editor read-only access and mobile overflow.
It uses an independent disposable publisher account and makes no external requests.
This seventh journey is wired into the existing full database runner. No assertion
was skipped or weakened. Initial strict lint findings were an unnecessary test mut
and identical confirmation branches; both were corrected rather than suppressed.

The broad run preceded a final occupied-member diagnostic: selected articles show
their current subject and a conflict links to that subject instead of silently
stealing its membership. The current restricted HTTP test and Chromium journey
were rerun successfully afterwards, including unchanged source/destination members.
Final unit/Clippy/build/runtime checks follow that change; GitHub performs the full
workspace/browser gates on the PR before merge.

Self-review covered all service/HTTP publisher fences, exact versions, explicit
unknown time, nonpersistent choice/merge preparation, fixed confirmation destination,
private audit visibility, caller-snapshot rejection and inverse state comparisons.
Prepared eligibility is never authority; final service transactions recheck it.
This is not independent review or full #29 article-publication acceptance. The
prepublication/review/publish/correct/withdraw interface and journey remain open.

No schema/dependency, provider, source HTTP, article-publication endpoint or live
operation. Rollback removes these consumers while retaining graph/audit history.
Local profile/cache/fixture settings follow earlier editor evidence; no host Docker
prune occurred. Unchanged frontend checks are not run locally in this small workspace;
GitHub workspace/frontend checks must pass before merge.
