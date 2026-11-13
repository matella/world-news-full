# Local admin verification

Scope: #15, stacked on HTTP PR #56. R2 source changes, new private identity schema
and local/disposable tests; no real accounts, grants, deployment or identity service.

Passed locally:

- Affected-crate unit/in-process suite: 23 tests. New coverage validates explicit
  writer configuration and same DB target, HTTPS/loopback restrictions, redacted
  settings, strict Origin/cookies/tokens and editor/publisher permission boundaries.
- `python3 scripts/test_database.py`: four fresh-DB suites passed (schema upgrade,
  importer, public HTTP and authentication). Authentication uses distinct runtime
  reader/writer roles with precisely bounded grants. Reader cannot read hashes;
  admin runtime cannot bootstrap identities or change roles.
- Synthetic bootstrap generated distinct salted Argon2id hashes for the same chosen
  test passphrase, rejected duplicates and weak input, and appended immutable audit.
  No embedded identity/default password or HTTP account-management route exists.
- Anonymous admin access, forged/duplicate cookies, unknown login, unexpected role
  form input, five-failure persistent lockout and ten-minute-expiry recovery tested.
- HTTPS cookie headers use Secure/__Host, HttpOnly, SameSite=Strict, Path=/, no
  Domain and eight-hour Max-Age. DB stores token/CSRF digests, not bearer values.
- Editor access is allowed while publisher-only access is denied; publishers can
  enter the protected capability view. Changing the DB role takes effect on the
  next request. This is capability verification, not an actual publish operation.
- Missing/null/cross-origin Origin, missing/incorrect/duplicate/another-session CSRF
  tokens fail. Valid protected logout clears cookies and makes replay fail. Expired
  sessions and disabled accounts fail; disabling revokes all of that user's sessions.
- A 31st admission attempt in a minute is throttled; an unavailable writer prevents
  login while the separately credentialed public reader still serves anonymous data.
- Real CLI and HTTP tests passed: bootstrap, duplicate refusal, Origin check, form
  sign-in, CSRF rejection/logout, disable, private logs and graceful shutdown.
- `WORLDNEWS_BROWSER_CHECK=1 python3 scripts/test_database.py` additionally passed
  Chromium form login/logout. Browser supplied the configured Origin, accepted
  HttpOnly/SameSite cookies and removed them on logout. No external browser requests.

Browser testing initially failed: no-referrer produced Origin:null on same-origin
form POSTs. A focused Chromium HTTP fixture reproduced this and confirmed that
same-origin preserves Origin while suppressing external referrers. The application
now uses that policy and the full browser test passed. The first DB auth run also
caught PostgreSQL CASE widening the attempt counter to integer; an explicit smallint
result now matches the typed row, and the full rerun passed.

Strict affected-crate Clippy/build, formatting, foundation/secret/diff guards and
real-process outage checks passed before PR opening. Full-workspace validation is
provided by GitHub CI. Test accounts/data/resources are synthetic and task-owned.

Not run: actual production HTTPS/proxy and grant validation, production identities,
comprehensive RustSec audit, or real edit/review/publish journeys. #16 must enforce
permissions and session revocation inside editorial transactions. #15 remains open
until those consuming workflows and the dependency stack are integrated. Admin
dashboard/publishing capability pages explicitly state that editing is unfinished.

See ADR 0003 and admin-auth.md for dependency choices, rate/lockout tradeoffs,
bootstrap/recovery and retention. Migration 4 has no destructive down operation.
After it is applied, retain a matching binary/forward fix or use a separately
verified restore; an older exact-version binary cannot safely serve the newer DB.
