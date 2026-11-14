# Local admin setup and session contract

Issue #15; architecture: ADR 0003. Admin defaults to disabled (503). No real users
are created by startup. Next.js transitional Basic-auth settings are separate.

## Authorised staging setup

1. Migrate the isolated target with the migration owner and `--migrate`.
2. Choose a unique lower-case username (3–64 ASCII letters/digits/underscore/dot/
   hyphen; first character a letter), role editor or publisher and a strong chosen
   passphrase (16+ characters, <=1,024 UTF-8 bytes, no control characters).
3. Inject the owner's DATABASE_URL outside Git. Supply the password through stdin
   from an approved secret-manager pipe or a restricted file descriptor, not argv,
   environment, an issue, logs or shell history:

   ```sh
   target/debug/world-news --create-admin chosen-editor editor < "$PASSWORD_INPUT_FILE"
   ```

   Input is a single UTF-8 passphrase with optional final LF/CRLF. The CLI refuses
   duplicate IDs and never updates an existing account or elevates its role.
4. Provision separate non-owner reader/admin runtime roles using the staging grant
   plan below. No application command automatically grants database privileges.
5. Run with DATABASE_URL set to the reader credential, EDITOR_DATABASE_URL set to
   the admin-runtime credential, NEWS_ADMIN_ENABLED=true, and PUBLIC_URL equal to
   the browser's exact external origin. Production requires HTTPS and verify-full
   DB TLS. Reader/writer URLs must identify the same host, port and database name.
6. Visit /admin/login. Public reading requires no login. Changing Host/Forwarded or
   passing role/query parameters does not authenticate or grant a role.

The CLI needs explicit destination credentials; these examples are not approval to
create a live account or modify production roles. Do not put owner credentials in
the long-running web environment. Startup does not migrate or bootstrap identities.

## Least-privilege staging grant plan

The reader needs USAGE on news and SELECT on public_* views plus schema_version.
The admin runtime needs USAGE on news; SELECT on schema_version, admin_users and
admin_sessions; UPDATE(failed_attempts,locked_until) on admin_users; INSERT on
admin_sessions and admin_audit; UPDATE(revoked_at) on admin_sessions; USAGE on
admin_audit_id_seq. It does **not** get INSERT users, UPDATE role/password/active,
DELETE audit or schema ownership. Apply no blanket ALL ON ALL TABLES/default grants.
The disposable integration suite provisions and proves these exact privileges.
Actual role names/secrets belong to the environment release plan, not Git.

Draft/editorial operations are still #16/#29. The authenticated dashboard identifies
the user/role and supports logout. /admin/publishing is a publisher-only capability
view, explicitly stating that the publication workflow is in preparation. It does
not publish anything. Source authorization primitives do not complete publishing.

## Request behaviour

- Admin disabled/misconfigured/unavailable: 503; public reading continues using its
  separate pool where possible. Anonymous/expired/revoked/invalid session: 401 with
  sign-in link and clearing cookies. Insufficient role/failed CSRF: 403. Invalid
  form: redacted 400. Admission throttled: 429 with Retry-After 60.
- Login form POST accepts username/password only; unknown/duplicate fields fail.
  Successful sign-in always redirects to /admin, logout to /admin/login. No return
  URL is accepted, including absolute/network-path values or forged proxy headers.
- Secure host-only cookies last at most eight hours. No username/role/password is
  stored in them. The two HttpOnly cookies must match a live session. They are
  SameSite=Strict and are cleared on logout; replay after revocation fails.
- Every protected mutation requires exactly the configured Origin and either one
  form csrf value or one X-CSRF-Token header. An empty/null/cross-origin Origin,
  duplicate token, or another session's token fails. Login also checks Origin.
  Nonbrowser automation must supply the exact Origin and session/CSRF values.
- Cookies and form values are never logged. All admin responses are no-store and
  noindex; fixed route IDs/status/timing remain available for diagnostics. No
  permissive CORS, role headers or trusted Forwarded/X-Forwarded-* shortcuts exist.
- Five wrong attempts lock an existing active account for ten minutes. Unknown,
  disabled and locked accounts receive the same failure and dummy-hash work. A
  separate per-process admission window permits 30 attempts/minute; it resets on
  restart and is not a distributed limit. Hash concurrency is at most two workers.

For local HTTP browser testing, PUBLIC_URL must use localhost/127.0.0.1/[::1] and
APP_ENV=development. This uses unprefixed, non-Secure cookies solely for loopback.
Production TLS termination must be verified separately; PUBLIC_URL is a configured
assertion, not proof of an encrypted connection. Use same-origin Referrer-Policy;
no-referrer turns Chromium's form Origin into null and fails the intended check.

## Revocation, recovery and retention

`world-news --disable-admin chosen-editor` with authorised operator credentials
atomically disables the identity, revokes all its sessions and appends an audit
record. This is explicit, never an HTTP publisher permission. For credential loss,
bootstrap a new uniquely named operator-approved identity and disable the old one;
do not delete old actor records or silently overwrite identity/history. Role or
password changes are not exposed by the current runtime/CLI; a reviewed operator
migration is needed if those become required.

Retain migration-owner recovery access independently of web sign-in. Known-account
lockout trades brute-force resistance for temporary denial risk. Expired/revoked
session rows and immutable audit need an environment retention/backup policy (#47);
no background deletion job or unreviewed production cleanup is shipped here.

Tests use disposable databases and synthetic accounts. Run `cargo test -p world-news
--locked`, build, then `python3 scripts/test_database.py`; add WORLDNEWS_BROWSER_CHECK=1
for installed Playwright/Chromium. Browser smoke covers loopback form Origin,
HttpOnly/SameSite/logout. In-process HTTPS tests check Secure/__Host attributes;
actual deployment TLS/proxy behaviour and full edit/review/publish remain #16/#48.
