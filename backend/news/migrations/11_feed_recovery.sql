ALTER TABLE news.feed_fetch_attempts ADD COLUMN lease_until timestamptz;
-- Historical started rows have no proven live owner. Make them eligible for
-- explicit recovery, rather than inventing a fresh lease during upgrade.
UPDATE news.feed_fetch_attempts SET lease_until=clock_timestamp() WHERE outcome='started';
ALTER TABLE news.feed_fetch_attempts ADD CHECK ((outcome='started')=(lease_until IS NOT NULL));
CREATE INDEX feed_expired_attempts ON news.feed_fetch_attempts(lease_until,id) WHERE outcome='started';
ALTER TABLE news.feeds
    ADD COLUMN fetch_control_version bigint NOT NULL DEFAULT 0 CHECK(fetch_control_version>=0),
    ADD COLUMN fetch_pause_reason text CHECK(fetch_pause_reason IN ('retry_after_requires_review','retry_budget_exhausted','manual_pause','legacy_retry_instruction_unknown')),
    ADD COLUMN retry_after_seconds text CHECK(retry_after_seconds ~ '^[0-9]{1,20}$'),
    ADD COLUMN retry_after_observed_at timestamptz,
    ADD CHECK((retry_after_seconds IS NULL)=(retry_after_observed_at IS NULL));
-- Old workers did not retain the original Retry-After value. Do not invent a
-- safe deadline for those holds or reset an exhausted historic failure budget.
UPDATE news.feeds SET
    fetch_pause_reason=CASE WHEN fetch_paused AND last_error='retry_after_requires_review' THEN 'legacy_retry_instruction_unknown'
        WHEN failure_count>=5 THEN 'retry_budget_exhausted' WHEN fetch_paused THEN 'manual_pause' ELSE NULL END,
    fetch_paused=fetch_paused OR failure_count>=5;
ALTER TABLE news.normalization_jobs ADD UNIQUE(id,batch_id);
CREATE TABLE news.normalization_job_origins (
    job_id bigint PRIMARY KEY,
    fetch_attempt_id bigint NOT NULL UNIQUE REFERENCES news.feed_batches(attempt_id),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY(job_id,fetch_attempt_id) REFERENCES news.normalization_jobs(id,batch_id)
);
CREATE TRIGGER immutable_normalization_job_origin BEFORE UPDATE OR DELETE ON news.normalization_job_origins
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TABLE news.feed_operational_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    feed_id text NOT NULL REFERENCES news.feeds(id),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    action text NOT NULL CHECK(action IN ('paused','resumed')),
    reason text NOT NULL CHECK(length(btrim(reason)) BETWEEN 1 AND 2000),
    control_version bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER immutable_feed_operational_action BEFORE UPDATE OR DELETE ON news.feed_operational_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
