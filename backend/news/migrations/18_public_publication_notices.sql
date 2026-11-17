-- Lifecycle reasons are explicitly public; actors and private review/provenance stay private.
-- Keep the existing view columns/grants and add initial publication notices.
CREATE OR REPLACE VIEW news.public_notices WITH (security_barrier=true) AS
SELECT e.id,e.story_id,e.revision_id,e.action,e.public_reason,e.created_at
FROM news.publication_events e JOIN news.stories s ON s.id=e.story_id
WHERE s.state IN ('published','withdrawn') AND e.action IN ('published','corrected','withdrawn');
