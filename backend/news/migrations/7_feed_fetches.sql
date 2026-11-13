ALTER TABLE news.feeds
    ADD COLUMN validator_url text,
    ADD COLUMN last_attempt_at timestamptz,
    ADD COLUMN last_outcome text CHECK (last_outcome IN ('started','modified','not_modified','failed','cancelled')),
    ADD COLUMN last_error text,
    ADD COLUMN last_http_status integer CHECK (last_http_status BETWEEN 100 AND 599),
    ADD COLUMN failure_count integer NOT NULL DEFAULT 0 CHECK (failure_count BETWEEN 0 AND 32),
    ADD COLUMN fetch_paused boolean NOT NULL DEFAULT false;
CREATE TABLE news.feed_fetch_attempts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    feed_id text NOT NULL REFERENCES news.feeds(id),
    source_id text NOT NULL REFERENCES news.sources(id),
    feed_version bigint NOT NULL,
    source_version bigint NOT NULL,
    outcome text NOT NULL DEFAULT 'started' CHECK (outcome IN ('started','modified','not_modified','failed','cancelled')),
    error_category text,
    http_status integer CHECK (http_status BETWEEN 100 AND 599),
    response_bytes bigint CHECK (response_bytes >= 0),
    entry_count integer CHECK (entry_count BETWEEN 0 AND 500),
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    CHECK ((outcome='started') = (finished_at IS NULL))
);
ALTER TABLE news.feeds ADD COLUMN last_attempt_id bigint REFERENCES news.feed_fetch_attempts(id);
CREATE INDEX feed_fetch_history ON news.feed_fetch_attempts(feed_id,id DESC);
CREATE FUNCTION news.protect_finished_fetch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.outcome<>'started' OR (OLD.id,OLD.feed_id,OLD.source_id,OLD.feed_version,OLD.source_version,OLD.started_at)
        IS DISTINCT FROM (NEW.id,NEW.feed_id,NEW.source_id,NEW.feed_version,NEW.source_version,NEW.started_at) THEN
        RAISE EXCEPTION 'finished fetch/context is immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable_finished_fetch BEFORE UPDATE ON news.feed_fetch_attempts
    FOR EACH ROW EXECUTE FUNCTION news.protect_finished_fetch();
CREATE TABLE news.feed_batches (
    attempt_id bigint PRIMARY KEY REFERENCES news.feed_fetch_attempts(id),
    metadata jsonb NOT NULL CHECK (jsonb_typeof(metadata)='object' AND octet_length(metadata::text)<=4194304),
    content_hash text NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER immutable_feed_batch BEFORE UPDATE OR DELETE ON news.feed_batches
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
-- A fetch worker may hold a short share lock without receiving UPDATE authority
-- over registry configuration. Runtime grants must explicitly allow this function.
CREATE FUNCTION news.lock_source_for_fetch(selected_id text)
RETURNS TABLE(enabled boolean,lock_version bigint)
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,news,pg_temp AS $$
    SELECT s.enabled,s.lock_version FROM news.sources s WHERE s.id=selected_id FOR SHARE
$$;
REVOKE ALL ON FUNCTION news.lock_source_for_fetch(text) FROM PUBLIC;
