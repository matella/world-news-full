ALTER TABLE news.source_articles ADD COLUMN intake_version bigint NOT NULL DEFAULT 0 CHECK (intake_version>=0);
CREATE TABLE news.intake_snapshots (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_article_id text NOT NULL REFERENCES news.source_articles(id),
    version bigint NOT NULL CHECK(version>0),
    source_version bigint NOT NULL CHECK(source_version>=0),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    kind text NOT NULL CHECK(kind IN ('extracted','failed','denied','manual','registered_link')),
    error_category text,
    policy jsonb NOT NULL CHECK(jsonb_typeof(policy)='object'),
    extraction_metadata jsonb CHECK(jsonb_typeof(extraction_metadata)='object' AND NOT extraction_metadata ? 'paragraphs'),
    notes text CHECK(length(btrim(notes)) BETWEEN 1 AND 20000),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(source_article_id,version),
    UNIQUE(source_article_id,id),
    CHECK ((kind='extracted') = (extraction_metadata IS NOT NULL)),
    CHECK ((kind IN ('failed','denied')) = (error_category IS NOT NULL))
);
ALTER TABLE news.source_articles ADD COLUMN current_intake_id bigint,
    ADD FOREIGN KEY(id,current_intake_id) REFERENCES news.intake_snapshots(source_article_id,id);
CREATE TABLE news.intake_draft_links (
    snapshot_id bigint PRIMARY KEY REFERENCES news.intake_snapshots(id),
    story_id text NOT NULL,
    initial_revision_id text NOT NULL,
    FOREIGN KEY(story_id,initial_revision_id) REFERENCES news.story_revisions(story_id,id)
);
CREATE INDEX intake_history ON news.intake_snapshots(source_article_id,id DESC);
CREATE TRIGGER immutable_intake_snapshot BEFORE UPDATE OR DELETE ON news.intake_snapshots
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TRIGGER immutable_intake_draft_link BEFORE UPDATE OR DELETE ON news.intake_draft_links
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();

-- Retained publisher text is separate from immutable provenance so a policy
-- revocation can remove bytes without erasing the extraction/audit history.
CREATE TABLE news.intake_text (
    snapshot_id bigint PRIMARY KEY REFERENCES news.intake_snapshots(id),
    extracted jsonb NOT NULL CHECK(jsonb_typeof(extracted)='object' AND octet_length(extracted::text)<=1048576)
);
CREATE TRIGGER immutable_intake_text BEFORE UPDATE ON news.intake_text
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE TABLE news.intake_text_removals (
    snapshot_id bigint PRIMARY KEY REFERENCES news.intake_snapshots(id),
    source_id text NOT NULL REFERENCES news.sources(id),
    source_version bigint NOT NULL,
    reason text NOT NULL CHECK(reason='source_storage_policy_revoked'),
    removed_at timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER immutable_intake_text_removal BEFORE UPDATE OR DELETE ON news.intake_text_removals
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
CREATE FUNCTION news.purge_disallowed_intake_text() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,news,pg_temp AS $$
BEGIN
    IF NEW.rights_status='unknown' OR NEW.usage_policy='metadata_only'
        OR NEW.rights_note IS NULL OR length(btrim(NEW.rights_note))=0
        OR (NEW.rights_status='documented' AND (NEW.usage_policy<>'licensed_text' OR NEW.terms_url IS NULL))
        OR (NEW.rights_status='original' AND NEW.usage_policy<>'original') THEN
        WITH removed AS (
            DELETE FROM news.intake_text t USING news.intake_snapshots i,news.source_articles a
            WHERE t.snapshot_id=i.id AND i.source_article_id=a.id AND a.source_id=NEW.id
            RETURNING t.snapshot_id
        )
        INSERT INTO news.intake_text_removals(snapshot_id,source_id,source_version,reason)
            SELECT snapshot_id,NEW.id,NEW.lock_version,'source_storage_policy_revoked' FROM removed;
        UPDATE news.source_articles SET extraction_state='metadata_only'
            WHERE source_id=NEW.id AND current_intake_id IS NOT NULL;
    END IF;
    RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION news.purge_disallowed_intake_text() FROM PUBLIC;
CREATE TRIGGER purge_disallowed_intake_text AFTER UPDATE OF rights_status,usage_policy,rights_note,terms_url ON news.sources
    FOR EACH ROW EXECUTE FUNCTION news.purge_disallowed_intake_text();
