-- App-owned namespace. Never alter the unknown legacy/public tables here.
CREATE TABLE news.sources (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    name text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 300),
    homepage_url text NOT NULL CHECK (homepage_url ~ '^https?://'),
    enabled boolean NOT NULL DEFAULT true,
    ownership_note text,
    usage_policy text NOT NULL DEFAULT 'metadata_only' CHECK (usage_policy IN ('metadata_only','licensed_text','original')),
    terms_url text,
    metadata jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(metadata) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news.reporting_origins (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    name text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 300),
    source_id text REFERENCES news.sources(id),
    note text
);
CREATE TABLE news.feeds (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    source_id text NOT NULL REFERENCES news.sources(id),
    url text NOT NULL UNIQUE CHECK (url ~ '^https?://'),
    enabled boolean NOT NULL DEFAULT true,
    poll_seconds integer NOT NULL DEFAULT 3600 CHECK (poll_seconds >= 60),
    etag text,
    last_modified text,
    last_success_at timestamptz,
    next_fetch_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news.source_articles (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    source_id text NOT NULL REFERENCES news.sources(id),
    feed_id text REFERENCES news.feeds(id),
    canonical_key text NOT NULL UNIQUE CHECK (length(canonical_key) BETWEEN 1 AND 500),
    original_url text NOT NULL CHECK (original_url ~ '^https?://'),
    fetched_url text,
    title text NOT NULL,
    language text NOT NULL DEFAULT 'und' CHECK (length(language) BETWEEN 2 AND 35),
    body text,
    usage_policy text NOT NULL DEFAULT 'metadata_only' CHECK (usage_policy IN ('metadata_only','licensed_text','original')),
    extraction_state text NOT NULL DEFAULT 'pending' CHECK (extraction_state IN ('pending','ready','failed','metadata_only')),
    published_at timestamptz,
    fetched_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(metadata) = 'object'),
    CHECK (usage_policy <> 'metadata_only' OR body IS NULL)
);
CREATE INDEX source_articles_source_date ON news.source_articles(source_id, published_at DESC);
CREATE TABLE news.source_article_origins (
    source_article_id text NOT NULL REFERENCES news.source_articles(id),
    origin_id text NOT NULL REFERENCES news.reporting_origins(id),
    PRIMARY KEY (source_article_id, origin_id)
);
CREATE TABLE news.threads (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    title text NOT NULL,
    state text NOT NULL DEFAULT 'active' CHECK (state IN ('active','closed')),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news.stories (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    state text NOT NULL DEFAULT 'draft' CHECK (state IN ('draft','in_review','published','withdrawn')),
    current_revision_id text,
    published_revision_id text,
    lock_version bigint NOT NULL DEFAULT 0 CHECK (lock_version >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (state NOT IN ('published','withdrawn') OR published_revision_id IS NOT NULL)
);
CREATE TABLE news.story_revisions (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    story_id text NOT NULL REFERENCES news.stories(id),
    number integer NOT NULL CHECK (number > 0),
    title text NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 500),
    standfirst text,
    summary text,
    body jsonb NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(body) = 'array'),
    language text NOT NULL CHECK (length(language) BETWEEN 2 AND 35),
    format text NOT NULL CHECK (format IN ('brief','standard','analysis')),
    kind text NOT NULL CHECK (kind IN ('reporting','analysis','opinion','automated_summary','legacy_summary','legacy_synthesis')),
    topics text[] NOT NULL DEFAULT '{}',
    geography text[] NOT NULL DEFAULT '{}',
    actor_id text NOT NULL CHECK (length(btrim(actor_id)) > 0),
    change_reason text,
    provenance jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(provenance) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (story_id, number),
    UNIQUE (story_id, id)
);
ALTER TABLE news.stories
    ADD FOREIGN KEY (id,current_revision_id) REFERENCES news.story_revisions(story_id,id) DEFERRABLE INITIALLY DEFERRED,
    ADD FOREIGN KEY (id,published_revision_id) REFERENCES news.story_revisions(story_id,id) DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE news.thread_stories (
    thread_id text NOT NULL REFERENCES news.threads(id),
    story_id text NOT NULL REFERENCES news.stories(id),
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (thread_id,story_id)
);
CREATE INDEX thread_stories_story ON news.thread_stories(story_id);
CREATE TABLE news.citations (
    revision_id text NOT NULL REFERENCES news.story_revisions(id),
    claim_id text NOT NULL CHECK (length(btrim(claim_id)) BETWEEN 1 AND 200),
    source_article_id text NOT NULL REFERENCES news.source_articles(id),
    location text,
    PRIMARY KEY (revision_id,claim_id,source_article_id)
);
CREATE TABLE news.translations (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    source_revision_id text NOT NULL REFERENCES news.story_revisions(id),
    language text NOT NULL CHECK (length(language) BETWEEN 2 AND 35),
    number integer NOT NULL CHECK (number > 0),
    title text NOT NULL,
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'array'),
    disagreements jsonb NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(disagreements) = 'array'),
    method text NOT NULL CHECK (method IN ('manual','assisted','legacy')),
    review_state text NOT NULL DEFAULT 'draft' CHECK (review_state IN ('draft','reviewed','legacy_unverified')),
    actor_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_revision_id,language,number)
);
CREATE TABLE news.editorial_reviews (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    revision_id text NOT NULL REFERENCES news.story_revisions(id),
    actor_id text NOT NULL,
    outcome text NOT NULL CHECK (outcome IN ('approved','changes_requested')),
    internal_note text,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news.publication_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    story_id text NOT NULL REFERENCES news.stories(id),
    revision_id text NOT NULL,
    action text NOT NULL CHECK (action IN ('published','corrected','withdrawn','imported')),
    public_reason text NOT NULL CHECK (length(btrim(public_reason)) > 0),
    actor_id text NOT NULL CHECK (length(btrim(actor_id)) > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (story_id,revision_id) REFERENCES news.story_revisions(story_id,id)
);
CREATE INDEX publication_events_story_date ON news.publication_events(story_id,created_at DESC,id DESC);
CREATE INDEX publication_events_revision ON news.publication_events(revision_id,action);
CREATE TABLE news.jobs (
    id text PRIMARY KEY DEFAULT gen_random_uuid()::text CHECK (length(id) BETWEEN 1 AND 200),
    kind text NOT NULL,
    state text NOT NULL DEFAULT 'queued' CHECK (state IN ('queued','running','completed','failed','cancelled')),
    payload jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(payload) = 'object'),
    idempotency_key text UNIQUE,
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts integer NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    run_after timestamptz NOT NULL DEFAULT now(),
    locked_by text,
    lease_expires_at timestamptz,
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (attempts <= max_attempts),
    CHECK (state <> 'running' OR (locked_by IS NOT NULL AND lease_expires_at IS NOT NULL))
);
CREATE INDEX jobs_claim ON news.jobs(run_after,id) WHERE state = 'queued';
CREATE TABLE news.legacy_records (
    legacy_id text NOT NULL,
    content_hash text NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    story_id text NOT NULL,
    revision_id text NOT NULL,
    original jsonb NOT NULL CHECK (jsonb_typeof(original) = 'object'),
    imported_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (legacy_id,content_hash),
    FOREIGN KEY (story_id,revision_id) REFERENCES news.story_revisions(story_id,id)
);
CREATE TABLE news.legacy_links (
    story_id text NOT NULL REFERENCES news.stories(id),
    related_legacy_id text NOT NULL,
    PRIMARY KEY (story_id,related_legacy_id)
);

CREATE FUNCTION news.reject_immutable_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'immutable record: append a new revision or event' USING ERRCODE = '23514';
END $$;
CREATE TRIGGER immutable_revision BEFORE UPDATE OR DELETE ON news.story_revisions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_publication_event BEFORE UPDATE OR DELETE ON news.publication_events
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_review BEFORE UPDATE OR DELETE ON news.editorial_reviews
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_translation BEFORE UPDATE OR DELETE ON news.translations
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_legacy_record BEFORE UPDATE OR DELETE ON news.legacy_records
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();

CREATE FUNCTION news.require_publication_event() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    latest_action text;
    latest_revision text;
BEGIN
    IF NEW.state IN ('published','withdrawn') THEN
        SELECT action,revision_id INTO latest_action,latest_revision FROM news.publication_events
            WHERE story_id=NEW.id ORDER BY id DESC LIMIT 1;
        IF latest_revision IS DISTINCT FROM NEW.published_revision_id
            OR (NEW.state='published' AND latest_action NOT IN ('published','corrected','imported'))
            OR (NEW.state='withdrawn' AND latest_action IS DISTINCT FROM 'withdrawn') THEN
            RAISE EXCEPTION 'publication requires a matching audit event' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER publication_has_event BEFORE INSERT OR UPDATE ON news.stories
    FOR EACH ROW EXECUTE FUNCTION news.require_publication_event();

CREATE FUNCTION news.protect_published_citation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target text;
BEGIN
    -- Lock the owning story, serialising citation edits with publication.
    FOR target IN SELECT DISTINCT v FROM unnest(ARRAY[
        CASE WHEN TG_OP <> 'INSERT' THEN OLD.revision_id END,
        CASE WHEN TG_OP <> 'DELETE' THEN NEW.revision_id END]) v WHERE v IS NOT NULL ORDER BY v
    LOOP
        PERFORM 1 FROM news.stories s JOIN news.story_revisions r ON r.story_id=s.id
            WHERE r.id=target FOR UPDATE OF s;
        IF EXISTS (SELECT 1 FROM news.publication_events WHERE revision_id=target
            AND action IN ('published','corrected','imported')) THEN
            RAISE EXCEPTION 'published citations are immutable' USING ERRCODE = '23514';
        END IF;
    END LOOP;
    IF TG_OP = 'DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER immutable_published_citation BEFORE INSERT OR UPDATE OR DELETE ON news.citations
    FOR EACH ROW EXECUTE FUNCTION news.protect_published_citation();
