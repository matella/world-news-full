CREATE TABLE news.extraction_jobs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_article_id text NOT NULL REFERENCES news.source_articles(id),
    expected_intake_version bigint NOT NULL CHECK(expected_intake_version>=0),
    source_version bigint NOT NULL CHECK(source_version>=0),
    requested_by text NOT NULL REFERENCES news.admin_users(id),
    authorized_until timestamptz NOT NULL DEFAULT clock_timestamp()+interval '1 hour',
    state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','retry','succeeded','failed','cancelled')),
    generation bigint NOT NULL DEFAULT 0 CHECK(generation>=0),
    attempt_count integer NOT NULL DEFAULT 0 CHECK(attempt_count BETWEEN 0 AND 3),
    lock_version bigint NOT NULL DEFAULT 0 CHECK(lock_version>=0),
    available_at timestamptz NOT NULL DEFAULT now(),
    lease_until timestamptz,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK((state='running')=(lease_until IS NOT NULL)),
    UNIQUE(source_article_id,id)
);
CREATE UNIQUE INDEX extraction_active_grant ON news.extraction_jobs(source_article_id,expected_intake_version) WHERE state IN ('queued','running','retry');
CREATE INDEX extraction_due ON news.extraction_jobs(available_at,id) WHERE state IN ('queued','retry');
CREATE INDEX extraction_expired ON news.extraction_jobs(lease_until,id) WHERE state='running';
CREATE TABLE news.extraction_attempts (
    job_id bigint NOT NULL REFERENCES news.extraction_jobs(id),
    generation bigint NOT NULL CHECK(generation>0),
    outcome text NOT NULL DEFAULT 'started' CHECK(outcome IN ('started','succeeded','retry','failed','cancelled','lease_expired')),
    error_category text,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    PRIMARY KEY(job_id,generation),
    CHECK((outcome='started')=(finished_at IS NULL))
);
CREATE TRIGGER immutable_finished_extraction_attempt BEFORE UPDATE OR DELETE ON news.extraction_attempts
    FOR EACH ROW EXECUTE FUNCTION news.protect_normalization_attempt();
CREATE TABLE news.extraction_job_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_id bigint NOT NULL REFERENCES news.extraction_jobs(id),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    action text NOT NULL CHECK(action IN ('requested','cancelled')),
    reason text NOT NULL CHECK(length(btrim(reason)) BETWEEN 1 AND 2000),
    job_version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER immutable_extraction_job_action BEFORE UPDATE OR DELETE ON news.extraction_job_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE FUNCTION news.protect_extraction_job_context() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN RAISE EXCEPTION 'job history cannot be deleted' USING ERRCODE='23514'; END IF;
    IF (OLD.id,OLD.source_article_id,OLD.expected_intake_version,OLD.source_version,OLD.requested_by,OLD.authorized_until,OLD.created_at)
        IS DISTINCT FROM (NEW.id,NEW.source_article_id,NEW.expected_intake_version,NEW.source_version,NEW.requested_by,NEW.authorized_until,NEW.created_at) THEN
        RAISE EXCEPTION 'extraction grant is immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable_extraction_job_context BEFORE UPDATE OR DELETE ON news.extraction_jobs
    FOR EACH ROW EXECUTE FUNCTION news.protect_extraction_job_context();
ALTER TABLE news.intake_snapshots ALTER COLUMN actor_id DROP NOT NULL,
    ADD COLUMN worker_job_id bigint UNIQUE,
    ADD FOREIGN KEY(source_article_id,worker_job_id) REFERENCES news.extraction_jobs(source_article_id,id),
    ADD CHECK(num_nonnulls(actor_id,worker_job_id)=1),
    ADD CHECK(worker_job_id IS NULL OR kind IN ('extracted','failed'));
CREATE TABLE news.source_extraction_limits (
    source_id text PRIMARY KEY REFERENCES news.sources(id),
    retry_after_seconds text NOT NULL CHECK(retry_after_seconds ~ '^[0-9]{1,20}$'),
    observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    job_id bigint REFERENCES news.extraction_jobs(id),
    manual_snapshot_id bigint REFERENCES news.intake_snapshots(id),
    CHECK(num_nonnulls(job_id,manual_snapshot_id)=1)
);
-- Returns authority, not identity fields. Lock issuer before job/source/article,
-- matching operator revocation ordering without granting identity SELECT to workers.
CREATE FUNCTION news.lock_extraction_requester(selected_job bigint) RETURNS boolean
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,news,pg_temp AS $$
    SELECT u.active AND u.role='publisher' FROM news.admin_users u
    JOIN news.extraction_jobs j ON j.requested_by=u.id WHERE j.id=selected_job FOR SHARE OF u
$$;
REVOKE ALL ON FUNCTION news.lock_extraction_requester(bigint) FROM PUBLIC;
CREATE FUNCTION news.extraction_retry_allowed(selected_source text) RETURNS boolean
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,news,pg_temp AS $$
    SELECT NOT EXISTS(SELECT 1 FROM news.source_extraction_limits l WHERE l.source_id=selected_source
        AND extract(epoch FROM clock_timestamp()-l.observed_at)<l.retry_after_seconds::numeric)
$$;
REVOKE ALL ON FUNCTION news.extraction_retry_allowed(text) FROM PUBLIC;
