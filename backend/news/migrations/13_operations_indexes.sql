-- Bounded keyset admin filters should not scan the whole intake archive.
CREATE INDEX source_article_state_cursor ON news.source_articles(extraction_state,id);
CREATE INDEX source_article_source_cursor ON news.source_articles(source_id,id);
CREATE INDEX normalization_state_cursor ON news.normalization_jobs(state,id DESC);
CREATE INDEX extraction_state_cursor ON news.extraction_jobs(state,id DESC);
