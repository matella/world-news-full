CREATE TABLE news.normalization_jobs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    batch_id bigint NOT NULL UNIQUE REFERENCES news.feed_batches(attempt_id),
    state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','retry','succeeded','failed','cancelled')),
    generation bigint NOT NULL DEFAULT 0 CHECK(generation>=0),
    attempt_count integer NOT NULL DEFAULT 0 CHECK(attempt_count BETWEEN 0 AND 5),
    max_attempts integer NOT NULL CHECK(max_attempts BETWEEN 1 AND 5 AND attempt_count<=max_attempts),
    replay_count bigint NOT NULL DEFAULT 0 CHECK(replay_count>=0),
    lock_version bigint NOT NULL DEFAULT 0 CHECK(lock_version>=0),
    available_at timestamptz NOT NULL DEFAULT now(),
    lease_until timestamptz,
    last_error text CHECK(last_error IN ('lease_expired','held_disabled','invalid_data','conflict','forbidden','unavailable','constraint')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK((state='running') = (lease_until IS NOT NULL))
);
CREATE INDEX normalization_job_due ON news.normalization_jobs(available_at,id) WHERE state IN ('queued','retry');
CREATE INDEX normalization_job_expiry ON news.normalization_jobs(lease_until,id) WHERE state='running';
CREATE TABLE news.normalization_attempts (
    job_id bigint NOT NULL REFERENCES news.normalization_jobs(id),
    generation bigint NOT NULL CHECK(generation>0),
    outcome text NOT NULL DEFAULT 'started' CHECK(outcome IN ('started','succeeded','retry','failed','cancelled','lease_expired')),
    error_category text CHECK(error_category IN ('lease_expired','held_disabled','invalid_data','conflict','forbidden','unavailable','constraint')),
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    PRIMARY KEY(job_id,generation),
    CHECK((outcome='started') = (finished_at IS NULL))
);
CREATE TABLE news.normalization_job_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_id bigint NOT NULL REFERENCES news.normalization_jobs(id),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    action text NOT NULL CHECK(action IN ('enqueued','replayed','cancelled')),
    reason text NOT NULL CHECK(length(btrim(reason)) BETWEEN 1 AND 2000),
    job_version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER immutable_normalization_job_action BEFORE UPDATE OR DELETE ON news.normalization_job_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE FUNCTION news.protect_normalization_attempt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN RAISE EXCEPTION 'attempt history cannot be deleted' USING ERRCODE='23514'; END IF;
    IF OLD.outcome<>'started' OR (OLD.job_id,OLD.generation,OLD.started_at) IS DISTINCT FROM (NEW.job_id,NEW.generation,NEW.started_at) THEN
        RAISE EXCEPTION 'finished attempt/context is immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable_finished_normalization_attempt BEFORE UPDATE OR DELETE ON news.normalization_attempts
    FOR EACH ROW EXECUTE FUNCTION news.protect_normalization_attempt();
CREATE FUNCTION news.protect_normalization_job_context() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN RAISE EXCEPTION 'job history cannot be deleted' USING ERRCODE='23514'; END IF;
    IF (OLD.id,OLD.batch_id,OLD.max_attempts,OLD.created_at) IS DISTINCT FROM (NEW.id,NEW.batch_id,NEW.max_attempts,NEW.created_at) THEN
        RAISE EXCEPTION 'job context is immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable_normalization_job_context BEFORE UPDATE OR DELETE ON news.normalization_jobs
    FOR EACH ROW EXECUTE FUNCTION news.protect_normalization_job_context();
