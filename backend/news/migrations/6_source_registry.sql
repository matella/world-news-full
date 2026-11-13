ALTER TABLE news.sources ALTER COLUMN enabled SET DEFAULT false;
ALTER TABLE news.feeds ALTER COLUMN enabled SET DEFAULT false;
ALTER TABLE news.sources
    ADD COLUMN languages text[] NOT NULL DEFAULT '{}',
    ADD COLUMN geographic_coverage text[] NOT NULL DEFAULT '{}',
    ADD COLUMN attribution text,
    ADD COLUMN editorial_note text,
    ADD COLUMN rights_status text NOT NULL DEFAULT 'unknown' CHECK (rights_status IN ('unknown','documented','original')),
    ADD COLUMN display_policy text NOT NULL DEFAULT 'link_only' CHECK (display_policy IN ('link_only','summary','full_text')),
    ADD COLUMN rights_note text,
    ADD COLUMN lock_version bigint NOT NULL DEFAULT 0 CHECK (lock_version >= 0);
ALTER TABLE news.feeds
    ADD COLUMN max_response_bytes integer NOT NULL DEFAULT 2097152 CHECK (max_response_bytes BETWEEN 16384 AND 10485760),
    ADD COLUMN lock_version bigint NOT NULL DEFAULT 0 CHECK (lock_version >= 0);
ALTER TABLE news.reporting_origins
    ADD COLUMN enabled boolean NOT NULL DEFAULT true,
    ADD COLUMN lock_version bigint NOT NULL DEFAULT 0 CHECK (lock_version >= 0);
CREATE TABLE news.source_origin_links (
    source_id text NOT NULL REFERENCES news.sources(id),
    origin_id text NOT NULL REFERENCES news.reporting_origins(id),
    PRIMARY KEY(source_id,origin_id)
);
CREATE TABLE news.registry_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    entity_type text NOT NULL CHECK (entity_type IN ('source','feed','origin')),
    entity_id text NOT NULL,
    lock_version bigint NOT NULL CHECK (lock_version >= 0),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    before_record jsonb,
    after_record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(entity_type,entity_id,lock_version)
);
CREATE TRIGGER immutable_registry_action BEFORE UPDATE OR DELETE ON news.registry_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
-- Capture the currently stored label, without certifying its historical accuracy.
ALTER TABLE news.source_articles ADD COLUMN attribution_name text;
UPDATE news.source_articles a SET attribution_name=s.name FROM news.sources s WHERE s.id=a.source_id;
ALTER TABLE news.source_articles ALTER COLUMN attribution_name SET NOT NULL;
CREATE FUNCTION news.capture_source_attribution() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    SELECT name INTO NEW.attribution_name FROM news.sources WHERE id=NEW.source_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'source registry record missing' USING ERRCODE='23503'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER capture_source_attribution BEFORE INSERT ON news.source_articles
    FOR EACH ROW EXECUTE FUNCTION news.capture_source_attribution();
CREATE FUNCTION news.protect_registry_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME='source_articles' THEN
        IF OLD.attribution_name IS DISTINCT FROM NEW.attribution_name THEN
            RAISE EXCEPTION 'captured attribution is immutable' USING ERRCODE='23514';
        END IF;
    ELSIF TG_TABLE_NAME='feeds' THEN
        IF (OLD.id,OLD.source_id,OLD.url) IS DISTINCT FROM (NEW.id,NEW.source_id,NEW.url) THEN
            RAISE EXCEPTION 'feed identity is immutable; configure a new feed' USING ERRCODE='23514';
        END IF;
    ELSIF (OLD.id,OLD.name,OLD.source_id) IS DISTINCT FROM (NEW.id,NEW.name,NEW.source_id) THEN
        RAISE EXCEPTION 'reporting-origin identity is immutable; configure a new origin' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable_source_attribution BEFORE UPDATE ON news.source_articles
    FOR EACH ROW EXECUTE FUNCTION news.protect_registry_identity();
CREATE TRIGGER immutable_feed_identity BEFORE UPDATE ON news.feeds
    FOR EACH ROW EXECUTE FUNCTION news.protect_registry_identity();
CREATE TRIGGER immutable_used_origin BEFORE UPDATE ON news.reporting_origins
    FOR EACH ROW EXECUTE FUNCTION news.protect_registry_identity();
CREATE OR REPLACE VIEW news.public_citations WITH (security_barrier=true) AS
SELECT c.revision_id,c.claim_id,c.location,a.original_url,a.source_id,a.attribution_name AS source_name
FROM news.citations c JOIN news.public_revisions r ON r.id=c.revision_id
JOIN news.source_articles a ON a.id=c.source_article_id;
