-- Review belongs to the candidate revision, independently of public visibility.
-- Existing legacy content gains no invented approval or editor identity.
ALTER TABLE news.stories ADD COLUMN review_state text NOT NULL DEFAULT 'draft'
    CHECK (review_state IN ('draft','in_review','approved','changes_requested'));
CREATE TABLE news.editorial_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    story_id text NOT NULL REFERENCES news.stories(id),
    revision_id text NOT NULL,
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    action text NOT NULL CHECK (action IN ('created','revised','submitted','approved','changes_requested','published','corrected','withdrawn')),
    reason text NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 2000),
    lock_version bigint NOT NULL CHECK (lock_version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (story_id,revision_id) REFERENCES news.story_revisions(story_id,id),
    UNIQUE (story_id,lock_version)
);
CREATE INDEX editorial_actions_story ON news.editorial_actions(story_id,id DESC);
CREATE TRIGGER immutable_editorial_action BEFORE UPDATE OR DELETE ON news.editorial_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
