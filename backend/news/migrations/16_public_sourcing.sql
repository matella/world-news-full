-- Pure projection: explicit public field allowlist, no table or privileged access.
CREATE FUNCTION news.project_sourcing(package jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog AS $$
SELECT CASE WHEN jsonb_typeof(package)='object' THEN jsonb_build_object(
    'statement',CASE WHEN jsonb_typeof(package->'statement')='object' THEN jsonb_build_object(
        'mode',package->'statement'->>'mode',
        'attribution',package->'statement'->>'attribution',
        'limits',package->'statement'->>'limits') ELSE NULL END,
    'firsthand',(SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'method',f->>'method','attribution',f->>'attribution',
        'claim_ids',(SELECT COALESCE(jsonb_agg(c#>>'{}'),'[]') FROM jsonb_array_elements(
            CASE WHEN jsonb_typeof(f->'claim_ids')='array' THEN f->'claim_ids' ELSE '[]' END) c
            WHERE jsonb_typeof(c)='string'))),'[]')
        FROM jsonb_array_elements(CASE WHEN jsonb_typeof(package->'firsthand')='array' THEN package->'firsthand' ELSE '[]' END) f),
    'disagreements',(SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'subject',d->>'subject','positions',(SELECT COALESCE(jsonb_agg(jsonb_build_object(
            'claim_id',p->>'claim_id','attribution',p->>'attribution')),'[]')
            FROM jsonb_array_elements(CASE WHEN jsonb_typeof(d->'positions')='array' THEN d->'positions' ELSE '[]' END) p))),'[]')
        FROM jsonb_array_elements(CASE WHEN jsonb_typeof(package->'disagreements')='array' THEN package->'disagreements' ELSE '[]' END) d)
) ELSE NULL END
$$;
CREATE OR REPLACE VIEW news.public_revisions WITH (security_barrier=true) AS
SELECT r.id,r.story_id,r.title,r.standfirst,r.summary,r.body,r.language,r.format,r.kind,
       r.topics,r.geography,r.created_at,
       news.project_sourcing(CASE WHEN r.provenance#>>'{sourcing,schema}'='1'
           THEN r.provenance#>'{sourcing,package}' ELSE NULL END) AS sourcing
FROM news.story_revisions r JOIN news.stories s ON s.id=r.story_id
WHERE s.state='published' AND (r.id=s.published_revision_id OR EXISTS (
    SELECT 1 FROM news.edition_entries entry JOIN news.editions ed ON ed.id=entry.edition_id
    WHERE entry.revision_id=r.id AND ed.state='published'
));

-- Lock evidence before the story head: intake already uses article -> story order.
CREATE FUNCTION news.lock_citation_evidence(chosen_revision text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,news AS $$
BEGIN
    IF length(chosen_revision)>200 OR (SELECT count(*) FROM news.citations WHERE revision_id=chosen_revision)>200 THEN
        RAISE EXCEPTION 'citation evidence exceeds the publication bound' USING ERRCODE='23514';
    END IF;
    PERFORM a.id FROM news.source_articles a WHERE a.id IN (
        SELECT source_article_id FROM news.citations WHERE revision_id=chosen_revision
    ) ORDER BY a.id FOR SHARE;
    PERFORM o.id FROM news.reporting_origins o WHERE o.id IN (
        SELECT l.origin_id FROM news.article_origin_labels l JOIN news.citations c
        ON c.origin_assertion_id=l.assertion_id WHERE c.revision_id=chosen_revision
    ) ORDER BY o.id FOR SHARE;
END $$;
REVOKE ALL ON FUNCTION news.lock_citation_evidence(text) FROM PUBLIC;
