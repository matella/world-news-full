-- Imported identities and published memberships are retained, without fabricated actors/fences.
ALTER TABLE news.editions ADD COLUMN lock_version bigint NOT NULL DEFAULT 0 CHECK (lock_version >= 0);
ALTER TABLE news.edition_entries ADD COLUMN prepared_story_version bigint CHECK (prepared_story_version >= 0);
CREATE TABLE news.edition_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    edition_id text NOT NULL REFERENCES news.editions(id),
    actor_id text NOT NULL REFERENCES news.admin_users(id),
    action text NOT NULL CHECK (action IN ('created','saved','published')),
    reason text NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 2000),
    lock_version bigint NOT NULL CHECK (lock_version > 0),
    before_state jsonb,
    after_state jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (edition_id,lock_version)
);
CREATE INDEX edition_actions_history ON news.edition_actions(edition_id,id DESC);
CREATE TRIGGER immutable_edition_action BEFORE UPDATE OR DELETE ON news.edition_actions
    FOR EACH ROW EXECUTE FUNCTION news.reject_immutable_change();
