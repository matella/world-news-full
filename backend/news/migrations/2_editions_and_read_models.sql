CREATE TABLE news.editions (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    edition_date date NOT NULL,
    publication_timezone text NOT NULL CHECK (length(btrim(publication_timezone)) > 0),
    state text NOT NULL DEFAULT 'draft' CHECK (state IN ('draft','published')),
    published_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (edition_date,publication_timezone),
    CHECK ((state = 'published') = (published_at IS NOT NULL))
);
CREATE INDEX editions_published_date ON news.editions(edition_date DESC,id) WHERE state='published';
CREATE TABLE news.edition_entries (
    edition_id text NOT NULL REFERENCES news.editions(id),
    story_id text NOT NULL REFERENCES news.stories(id),
    revision_id text NOT NULL,
    section text NOT NULL CHECK (length(btrim(section)) BETWEEN 1 AND 100),
    position integer NOT NULL CHECK (position >= 0),
    is_lead boolean NOT NULL DEFAULT false,
    PRIMARY KEY (edition_id,story_id),
    UNIQUE (edition_id,position),
    FOREIGN KEY (story_id,revision_id) REFERENCES news.story_revisions(story_id,id)
);
CREATE UNIQUE INDEX one_edition_lead ON news.edition_entries(edition_id) WHERE is_lead;
CREATE INDEX edition_entries_revision ON news.edition_entries(revision_id);

CREATE FUNCTION news.protect_edition_entry() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target text;
    status text;
BEGIN
    FOR target IN SELECT DISTINCT v FROM unnest(ARRAY[
        CASE WHEN TG_OP <> 'INSERT' THEN OLD.edition_id END,
        CASE WHEN TG_OP <> 'DELETE' THEN NEW.edition_id END]) v WHERE v IS NOT NULL ORDER BY v
    LOOP
        SELECT state INTO status FROM news.editions WHERE id=target FOR UPDATE;
        IF status='published' THEN
            RAISE EXCEPTION 'published edition membership is immutable' USING ERRCODE = '23514';
        END IF;
    END LOOP;
    IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER immutable_edition_entry BEFORE INSERT OR UPDATE OR DELETE ON news.edition_entries
    FOR EACH ROW EXECUTE FUNCTION news.protect_edition_entry();

CREATE FUNCTION news.protect_edition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' AND OLD.state='published' THEN
        RAISE EXCEPTION 'published edition is immutable' USING ERRCODE = '23514';
    END IF;
    IF TG_OP <> 'DELETE' AND NEW.state='published' THEN
        IF NOT EXISTS (SELECT 1 FROM news.edition_entries WHERE edition_id=NEW.id) OR EXISTS (
            SELECT 1 FROM news.edition_entries entry WHERE entry.edition_id=NEW.id AND NOT EXISTS (
                SELECT 1 FROM news.publication_events event WHERE event.story_id=entry.story_id
                AND event.revision_id=entry.revision_id AND event.action IN ('published','corrected','imported')
            )
        ) THEN
            RAISE EXCEPTION 'edition requires previously published revisions' USING ERRCODE = '23514';
        END IF;
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER immutable_edition BEFORE INSERT OR UPDATE OR DELETE ON news.editions
    FOR EACH ROW EXECUTE FUNCTION news.protect_edition();

-- Grant readers SELECT only on these views; never on drafts, provenance or audit actors.
CREATE VIEW news.public_revisions WITH (security_barrier=true) AS
SELECT r.id,r.story_id,r.title,r.standfirst,r.summary,r.body,r.language,r.format,r.kind,
       r.topics,r.geography,r.created_at
FROM news.story_revisions r JOIN news.stories s ON s.id=r.story_id
WHERE s.state='published' AND (r.id=s.published_revision_id OR EXISTS (
    SELECT 1 FROM news.edition_entries entry JOIN news.editions ed ON ed.id=entry.edition_id
    WHERE entry.revision_id=r.id AND ed.state='published'
));
CREATE VIEW news.public_stories WITH (security_barrier=true) AS
SELECT s.id,r.id AS revision_id,r.title,r.standfirst,r.summary,r.body,r.language,r.format,r.kind,
       r.topics,r.geography,s.created_at,s.updated_at
FROM news.stories s JOIN news.public_revisions r ON r.id=s.published_revision_id;
CREATE VIEW news.public_editions WITH (security_barrier=true) AS
SELECT id,edition_date,publication_timezone,published_at FROM news.editions WHERE state='published';
CREATE VIEW news.public_edition_entries WITH (security_barrier=true) AS
SELECT entry.edition_id,entry.story_id,entry.revision_id,entry.section,entry.position,entry.is_lead,
       (s.state='withdrawn') AS withdrawn
FROM news.edition_entries entry JOIN news.editions ed ON ed.id=entry.edition_id
JOIN news.stories s ON s.id=entry.story_id WHERE ed.state='published';
CREATE VIEW news.public_citations WITH (security_barrier=true) AS
SELECT c.revision_id,c.claim_id,c.location,a.original_url,a.source_id,source.name AS source_name
FROM news.citations c JOIN news.public_revisions r ON r.id=c.revision_id
JOIN news.source_articles a ON a.id=c.source_article_id JOIN news.sources source ON source.id=a.source_id;
CREATE VIEW news.public_notices WITH (security_barrier=true) AS
SELECT e.id,e.story_id,e.revision_id,e.action,e.public_reason,e.created_at
FROM news.publication_events e JOIN news.stories s ON s.id=e.story_id
WHERE s.state IN ('published','withdrawn') AND e.action IN ('corrected','withdrawn');
CREATE VIEW news.public_translations WITH (security_barrier=true) AS
SELECT t.id,t.source_revision_id,t.language,t.number,t.title,t.body,t.disagreements,t.method,t.review_state,t.created_at
FROM news.translations t JOIN news.public_revisions r ON r.id=t.source_revision_id
WHERE t.review_state IN ('reviewed','legacy_unverified');
CREATE VIEW news.schema_version WITH (security_barrier=true) AS
SELECT version,checksum,success FROM news._sqlx_migrations;
