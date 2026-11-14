CREATE INDEX source_article_original_url_hash ON news.source_articles USING hash(original_url);
CREATE TABLE news.normalized_batches (
    attempt_id bigint PRIMARY KEY REFERENCES news.feed_batches(attempt_id),
    normalizer_version integer NOT NULL CHECK (normalizer_version=1),
    result jsonb NOT NULL CHECK (jsonb_typeof(result)='object'),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news.source_observations (
    attempt_id bigint NOT NULL REFERENCES news.normalized_batches(attempt_id) DEFERRABLE INITIALLY DEFERRED,
    entry_index integer NOT NULL CHECK (entry_index BETWEEN 0 AND 499),
    source_article_id text REFERENCES news.source_articles(id),
    disposition text NOT NULL CHECK (disposition IN ('inserted','duplicate','metadata_changed','quarantined')),
    reason text,
    original_entry jsonb NOT NULL CHECK (jsonb_typeof(original_entry)='object'),
    canonical_url text,
    canonical_key text,
    semantic_hash text CHECK (semantic_hash ~ '^[0-9a-f]{64}$'),
    observed_at timestamptz NOT NULL,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(attempt_id,entry_index)
);
CREATE INDEX source_observation_article ON news.source_observations(source_article_id,observed_at);
CREATE TRIGGER immutable_normalized_batch BEFORE UPDATE OR DELETE ON news.normalized_batches
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_source_observation BEFORE UPDATE OR DELETE ON news.source_observations
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE FUNCTION news.lock_feed_for_normalization(selected_id text)
RETURNS TABLE(enabled boolean,lock_version bigint,source_id text)
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,news,pg_temp AS $$
    SELECT f.enabled,f.lock_version,f.source_id FROM news.feeds f WHERE f.id=selected_id FOR SHARE
$$;
REVOKE ALL ON FUNCTION news.lock_feed_for_normalization(text) FROM PUBLIC;
