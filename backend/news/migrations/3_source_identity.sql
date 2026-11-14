-- A citation must not silently point elsewhere when extraction metadata changes.
-- Corrections use a new source-article record and a new story revision/citation.
CREATE FUNCTION news.protect_source_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (OLD.id,OLD.source_id,OLD.canonical_key,OLD.original_url) IS DISTINCT FROM
       (NEW.id,NEW.source_id,NEW.canonical_key,NEW.original_url) THEN
        RAISE EXCEPTION 'source identity is immutable; create a corrected source record' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable_source_identity BEFORE UPDATE ON news.source_articles
    FOR EACH ROW EXECUTE FUNCTION news.protect_source_identity();
