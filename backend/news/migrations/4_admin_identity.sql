-- Identity is private: public readers receive no grants on these base tables.
CREATE TABLE news.admin_users (
    id text PRIMARY KEY CHECK (id ~ '^[a-z][a-z0-9_.-]{2,63}$'),
    password_hash text NOT NULL CHECK (length(password_hash) BETWEEN 50 AND 512),
    role text NOT NULL CHECK (role IN ('editor','publisher')),
    active boolean NOT NULL DEFAULT true,
    failed_attempts smallint NOT NULL DEFAULT 0 CHECK (failed_attempts BETWEEN 0 AND 5),
    locked_until timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news.admin_sessions (
    token_hash text PRIMARY KEY CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    csrf_hash text NOT NULL CHECK (csrf_hash ~ '^[0-9a-f]{64}$'),
    user_id text NOT NULL REFERENCES news.admin_users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '8 hours')
);
CREATE INDEX live_admin_sessions ON news.admin_sessions(user_id,expires_at) WHERE revoked_at IS NULL;
CREATE TABLE news.admin_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id text REFERENCES news.admin_users(id),
    subject_id text REFERENCES news.admin_users(id),
    action text NOT NULL CHECK (action IN ('account_created','account_disabled','login_succeeded','login_failed','logout','authorization_denied')),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER immutable_admin_audit BEFORE UPDATE OR DELETE ON news.admin_audit
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
