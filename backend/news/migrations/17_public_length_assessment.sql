-- Only public assessment fields, never revision actors or unrelated provenance.
CREATE FUNCTION news.project_length_assessment(value jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog AS $$
SELECT CASE WHEN jsonb_typeof(value)='object' AND value->>'schema'='1' THEN jsonb_build_object(
    'schema',value->'schema','count_policy',value->>'count_policy',
    'policy_hash',value->>'policy_hash','words',value->'words',
    'language',value->>'language','format',value->>'format',
    'min_words',value->'min_words','max_words',value->'max_words',
    'supported',value->'supported','override_reason',value->>'override_reason'
) ELSE NULL END
$$;
CREATE OR REPLACE VIEW news.public_revisions WITH (security_barrier=true) AS
SELECT r.id,r.story_id,r.title,r.standfirst,r.summary,r.body,r.language,r.format,r.kind,
       r.topics,r.geography,r.created_at,
       news.project_sourcing(CASE WHEN r.provenance#>>'{sourcing,schema}'='1'
           THEN r.provenance#>'{sourcing,package}' ELSE NULL END) AS sourcing,
       news.project_length_assessment(r.provenance->'length') AS length_metadata
FROM news.story_revisions r JOIN news.stories s ON s.id=r.story_id
WHERE s.state='published' AND (r.id=s.published_revision_id OR EXISTS (
    SELECT 1 FROM news.edition_entries entry JOIN news.editions ed ON ed.id=entry.edition_id
    WHERE entry.revision_id=r.id AND ed.state='published'
));
