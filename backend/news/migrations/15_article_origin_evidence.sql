-- Article-specific reporting-origin assertions are separate from registry hints.
ALTER TABLE news.source_articles ADD COLUMN origin_version bigint NOT NULL DEFAULT 0 CHECK(origin_version>=0);
ALTER TABLE news.source_articles ADD COLUMN current_origin_assertion_id bigint;
CREATE TABLE news.article_origin_assertions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_article_id text NOT NULL REFERENCES news.source_articles(id),
    version bigint NOT NULL CHECK(version>0),
    source_version bigint NOT NULL CHECK(source_version>=0),
    state text NOT NULL CHECK(state IN ('unknown','documented','conflicting')),
    public_basis text NOT NULL CHECK(length(btrim(public_basis)) BETWEEN 1 AND 2000),
    private_note text NOT NULL CHECK(length(btrim(private_note)) BETWEEN 1 AND 10000),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(source_article_id,version), UNIQUE(source_article_id,version,id)
);
CREATE TABLE news.article_origin_labels (
    assertion_id bigint NOT NULL REFERENCES news.article_origin_assertions(id),
    origin_id text NOT NULL REFERENCES news.reporting_origins(id),
    captured_name text NOT NULL CHECK(length(btrim(captured_name)) BETWEEN 1 AND 300),
    PRIMARY KEY(assertion_id,origin_id)
);
ALTER TABLE news.source_articles ADD FOREIGN KEY(id,origin_version,current_origin_assertion_id)
    REFERENCES news.article_origin_assertions(source_article_id,version,id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE news.source_articles ADD CHECK((origin_version=0)=(current_origin_assertion_id IS NULL));
CREATE TRIGGER immutable_article_origin_assertion BEFORE UPDATE OR DELETE ON news.article_origin_assertions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_article_origin_label BEFORE UPDATE OR DELETE ON news.article_origin_labels
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE FUNCTION news.protect_article_origin_pointer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (OLD.origin_version,OLD.current_origin_assertion_id) IS DISTINCT FROM (NEW.origin_version,NEW.current_origin_assertion_id)
       AND (NEW.origin_version<>OLD.origin_version+1 OR NEW.current_origin_assertion_id IS NULL) THEN
        RAISE EXCEPTION 'origin assertions require a new monotonic evidence version' USING ERRCODE='23514';
    END IF;
    IF (OLD.origin_version,OLD.current_origin_assertion_id) IS DISTINCT FROM (NEW.origin_version,NEW.current_origin_assertion_id) THEN
        IF EXISTS(SELECT 1 FROM news.article_origin_assertions v WHERE v.id=NEW.current_origin_assertion_id AND
            ((v.state='documented' AND NOT EXISTS(SELECT 1 FROM news.article_origin_labels l WHERE l.assertion_id=v.id)) OR
             (v.state='unknown' AND EXISTS(SELECT 1 FROM news.article_origin_labels l WHERE l.assertion_id=v.id)))) THEN
            RAISE EXCEPTION 'origin evidence labels do not match the declared state' USING ERRCODE='23514';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER monotonic_article_origins BEFORE UPDATE ON news.source_articles
    FOR EACH ROW EXECUTE FUNCTION news.protect_article_origin_pointer();

CREATE FUNCTION news.prepare_article_origin_assertion() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_version bigint;
BEGIN
    SELECT origin_version INTO current_version FROM news.source_articles WHERE id=NEW.source_article_id FOR UPDATE;
    IF NOT FOUND OR NEW.version<>current_version+1 THEN
        RAISE EXCEPTION 'origin evidence must be the next article version' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER next_article_origin_assertion BEFORE INSERT ON news.article_origin_assertions
    FOR EACH ROW EXECUTE FUNCTION news.prepare_article_origin_assertion();
CREATE FUNCTION news.prepare_article_origin_label() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_version bigint; assertion_version bigint;
BEGIN
    SELECT a.origin_version,v.version INTO current_version,assertion_version
    FROM news.article_origin_assertions v JOIN news.source_articles a ON a.id=v.source_article_id
    WHERE v.id=NEW.assertion_id FOR SHARE OF a;
    IF NOT FOUND OR current_version>=assertion_version OR
       (SELECT count(*) FROM news.article_origin_labels WHERE assertion_id=NEW.assertion_id)>=20 THEN
        RAISE EXCEPTION 'origin evidence labels are sealed or exceed their bound' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER bounded_unsealed_origin_labels BEFORE INSERT ON news.article_origin_labels
    FOR EACH ROW EXECUTE FUNCTION news.prepare_article_origin_label();

ALTER TABLE news.citations ADD COLUMN anchor_id text CHECK(anchor_id IS NULL OR anchor_id ~ '^[A-Za-z0-9_-]{1,64}$');
ALTER TABLE news.citations ADD COLUMN origin_version bigint CHECK(origin_version>=0);
ALTER TABLE news.citations ADD COLUMN origin_assertion_id bigint;
ALTER TABLE news.citations ADD COLUMN origin_basis text;
ALTER TABLE news.citations ADD COLUMN origin_state text CHECK(origin_state IN ('unknown','documented','conflicting'));
ALTER TABLE news.citations ADD COLUMN origin_snapshot jsonb CHECK(jsonb_typeof(origin_snapshot)='array' AND jsonb_array_length(origin_snapshot)<=20);
-- The migration transaction holds the table DDL lock. Populate only new metadata;
-- do not infer claim spans or article-specific origins from legacy counts/hints.
DROP TRIGGER immutable_published_citation ON news.citations;
UPDATE news.citations SET origin_version=0,origin_state='unknown',origin_snapshot='[]';
ALTER TABLE news.citations ALTER COLUMN origin_version SET NOT NULL;
ALTER TABLE news.citations ALTER COLUMN origin_state SET NOT NULL;
ALTER TABLE news.citations ALTER COLUMN origin_snapshot SET NOT NULL;
ALTER TABLE news.citations ADD CHECK((origin_version=0)=(origin_assertion_id IS NULL));
ALTER TABLE news.citations ADD FOREIGN KEY(source_article_id,origin_version,origin_assertion_id)
    REFERENCES news.article_origin_assertions(source_article_id,version,id);
CREATE TRIGGER immutable_published_citation BEFORE INSERT OR UPDATE OR DELETE ON news.citations
    FOR EACH ROW EXECUTE FUNCTION news.protect_published_citation();
CREATE TRIGGER immutable_citation_snapshot BEFORE UPDATE OR DELETE ON news.citations
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE FUNCTION news.capture_citation_origin() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,news AS $$
DECLARE
    owner_story text;
    body jsonb;
    reviewing text;
    selected_version bigint;
BEGIN
    SELECT r.story_id,r.body,s.review_state INTO owner_story,body,reviewing
    FROM news.story_revisions r JOIN news.stories s ON s.id=r.story_id
    WHERE r.id=NEW.revision_id FOR UPDATE OF s;
    IF NOT FOUND THEN RAISE EXCEPTION 'citation revision missing' USING ERRCODE='23503'; END IF;
    IF EXISTS(SELECT 1 FROM news.editorial_reviews WHERE revision_id=NEW.revision_id)
       OR EXISTS(SELECT 1 FROM news.publication_events WHERE revision_id=NEW.revision_id) THEN
        RAISE EXCEPTION 'reviewed citations require a new revision' USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM news.stories WHERE id=owner_story AND current_revision_id=NEW.revision_id)
       AND reviewing<>'draft' THEN
        RAISE EXCEPTION 'citation changes must precede review' USING ERRCODE='23514';
    END IF;
    IF NEW.anchor_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM jsonb_array_elements(body) b WHERE
            (b->>'type'='claim' AND b->>'id'=NEW.anchor_id) OR
            (b->>'type'='section' AND EXISTS(SELECT 1 FROM jsonb_array_elements(COALESCE(b->'blocks','[]')) c
                WHERE c->>'type'='claim' AND c->>'id'=NEW.anchor_id))
    ) THEN RAISE EXCEPTION 'citation anchor is not in the body' USING ERRCODE='23514'; END IF;
    SELECT origin_version INTO selected_version FROM news.source_articles WHERE id=NEW.source_article_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'citation source missing' USING ERRCODE='23503'; END IF;
    NEW.origin_version:=COALESCE(NEW.origin_version,selected_version);
    IF NEW.origin_version>selected_version THEN
        RAISE EXCEPTION 'origin evidence is not finalized' USING ERRCODE='23514';
    END IF;
    IF NEW.origin_version=0 THEN
        NEW.origin_assertion_id:=NULL; NEW.origin_state:='unknown'; NEW.origin_snapshot:='[]'; NEW.origin_basis:=NULL;
    ELSE
        SELECT id,state,public_basis INTO NEW.origin_assertion_id,NEW.origin_state,NEW.origin_basis FROM news.article_origin_assertions
        WHERE source_article_id=NEW.source_article_id AND version=NEW.origin_version;
        IF NOT FOUND THEN RAISE EXCEPTION 'citation origin evidence missing' USING ERRCODE='23503'; END IF;
        SELECT COALESCE(jsonb_agg(jsonb_build_object('id',origin_id,'name',captured_name) ORDER BY origin_id),'[]'::jsonb)
        INTO NEW.origin_snapshot FROM news.article_origin_labels WHERE assertion_id=NEW.origin_assertion_id;
    END IF;
    IF EXISTS(SELECT 1 FROM news.citations WHERE revision_id=NEW.revision_id AND source_article_id=NEW.source_article_id
        AND origin_version<>NEW.origin_version) THEN
        RAISE EXCEPTION 'one revision must use one origin snapshot per source record' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION news.capture_citation_origin() FROM PUBLIC;
CREATE TRIGGER capture_citation_origin BEFORE INSERT ON news.citations
    FOR EACH ROW EXECUTE FUNCTION news.capture_citation_origin();
CREATE OR REPLACE VIEW news.public_citations WITH (security_barrier=true) AS
SELECT c.revision_id,c.claim_id,c.location,a.original_url,a.source_id,a.attribution_name AS source_name,
       c.anchor_id,c.source_article_id,c.origin_version,c.origin_state,c.origin_snapshot,c.origin_basis,
       (c.origin_version=a.origin_version AND NOT EXISTS(
           SELECT 1 FROM news.article_origin_labels l JOIN news.reporting_origins o ON o.id=l.origin_id
           WHERE l.assertion_id=c.origin_assertion_id AND NOT o.enabled)) AS origins_current
FROM news.citations c JOIN news.public_revisions r ON r.id=c.revision_id
JOIN news.source_articles a ON a.id=c.source_article_id;
-- Evidence curators need a shared registry lock, not registry UPDATE authority.
CREATE FUNCTION news.lock_reporting_origins_for_evidence(chosen_ids text[])
RETURNS TABLE(id text,name text,enabled boolean)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,news AS $$
BEGIN
    IF cardinality(chosen_ids)>20 OR array_position(chosen_ids,NULL) IS NOT NULL THEN
        RAISE EXCEPTION 'invalid bounded origin selection' USING ERRCODE='23514';
    END IF;
    RETURN QUERY SELECT o.id,o.name,o.enabled FROM news.reporting_origins o
        WHERE o.id=ANY(chosen_ids) ORDER BY o.id FOR SHARE;
END $$;
REVOKE ALL ON FUNCTION news.lock_reporting_origins_for_evidence(text[]) FROM PUBLIC;
