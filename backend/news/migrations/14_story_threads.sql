-- Primary subject identity is distinct from multi-valued topical classification.
-- Refuse ambiguous old assignments rather than silently selecting/deleting one.
DO $$ BEGIN
    IF EXISTS(SELECT 1 FROM news.thread_stories GROUP BY story_id HAVING count(*)>1)
       OR EXISTS(SELECT 1 FROM news.thread_stories GROUP BY thread_id HAVING count(*)>200) THEN
        RAISE EXCEPTION 'thread membership requires explicit reconciliation before upgrade' USING ERRCODE='23514';
    END IF;
END $$;
ALTER TABLE news.threads ADD COLUMN lock_version bigint NOT NULL DEFAULT 0 CHECK(lock_version>=0);
ALTER TABLE news.threads ADD COLUMN merged_into text REFERENCES news.threads(id);
ALTER TABLE news.threads ADD CHECK(length(btrim(title)) BETWEEN 1 AND 500);
ALTER TABLE news.threads ADD CHECK(merged_into IS NULL OR (merged_into<>id AND state='closed'));
ALTER TABLE news.thread_stories ADD COLUMN update_at timestamptz;
-- Existing added_at is bookkeeping, not a known development/publication instant.
CREATE UNIQUE INDEX one_primary_story_thread ON news.thread_stories(story_id);
CREATE INDEX thread_update_order ON news.thread_stories(thread_id,update_at DESC NULLS LAST,story_id);
CREATE TABLE news.thread_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    action text NOT NULL CHECK(action IN ('created','edited','member','removed','merged','split','undone')),
    reason text NOT NULL CHECK(length(btrim(reason)) BETWEEN 1 AND 2000),
    before_state jsonb NOT NULL CHECK(jsonb_typeof(before_state)='array' AND jsonb_array_length(before_state)<=2),
    after_state jsonb NOT NULL CHECK(jsonb_typeof(after_state)='array' AND jsonb_array_length(after_state) BETWEEN 1 AND 2),
    undo_of bigint UNIQUE REFERENCES news.thread_actions(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK((action='undone')=(undo_of IS NOT NULL))
);
CREATE INDEX thread_action_members ON news.thread_actions USING gin(after_state jsonb_path_ops);
CREATE TRIGGER immutable_thread_action BEFORE UPDATE OR DELETE ON news.thread_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();

CREATE VIEW news.public_thread_members WITH (security_barrier=true) AS
SELECT t.id AS thread_id,t.title AS thread_title,t.state,m.story_id,p.revision_id,
       p.title AS story_title,p.language,p.topics,p.geography,m.update_at
FROM news.threads t JOIN news.thread_stories m ON m.thread_id=t.id
JOIN news.public_stories p ON p.id=m.story_id
WHERE t.merged_into IS NULL;
CREATE VIEW news.public_thread_heads WITH (security_barrier=true) AS
WITH ranked AS (
    SELECT m.*,row_number() OVER(PARTITION BY thread_id ORDER BY update_at DESC NULLS LAST,story_id) AS rank
    FROM news.public_thread_members m
)
SELECT r.thread_id,r.thread_title,r.state,r.story_id AS head_story_id,
       r.revision_id AS head_revision_id,r.story_title AS head_title,r.update_at,
       (SELECT count(*) FROM news.public_thread_members m WHERE m.thread_id=r.thread_id) AS public_story_count,
       (SELECT count(DISTINCT e.edition_id) FROM news.public_thread_members m
        JOIN news.public_edition_entries e ON e.story_id=m.story_id
        WHERE m.thread_id=r.thread_id) AS edition_count,
       (SELECT count(DISTINCT ed.edition_date) FROM news.public_thread_members m
        JOIN news.public_edition_entries e ON e.story_id=m.story_id
        JOIN news.public_editions ed ON ed.id=e.edition_id
        WHERE m.thread_id=r.thread_id) AS date_count
FROM ranked r WHERE r.rank=1;
-- Resolve only to a currently public head. Fixed search path/no dynamic SQL;
-- parameterized bounded traversal avoids scanning all aliases for every read.
CREATE FUNCTION news.resolve_public_thread(wanted text) RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path=pg_catalog,news AS $$
    WITH RECURSIVE walk AS (
        SELECT id,merged_into,ARRAY[id] AS seen,0 AS depth FROM news.threads WHERE id=wanted
        UNION ALL
        SELECT t.id,t.merged_into,w.seen||t.id,w.depth+1 FROM walk w
        JOIN news.threads t ON t.id=w.merged_into
        WHERE w.depth<20 AND NOT t.id=ANY(w.seen)
    )
    SELECT w.id FROM walk w JOIN news.public_thread_heads h ON h.thread_id=w.id
    WHERE w.merged_into IS NULL LIMIT 1
$$;
REVOKE ALL ON FUNCTION news.resolve_public_thread(text) FROM PUBLIC;
