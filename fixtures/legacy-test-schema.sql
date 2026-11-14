-- Synthetic test schema inferred from current queries, NOT production migration DDL.
CREATE TABLE published_stories (
    id text PRIMARY KEY, title text NOT NULL, body text NOT NULL, lang text NOT NULL,
    title_fr text, body_fr text, topic text, origin_count integer NOT NULL,
    source_count integer NOT NULL, synthesized boolean, claims_json jsonb,
    disagreements_json jsonb, disagreements_fr jsonb, sources_json jsonb,
    edition_date date, related_ids text[], updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE news_status (id integer PRIMARY KEY, stats jsonb, updated_at timestamptz);
CREATE ROLE fixture_reader LOGIN PASSWORD 'synthetic-reader-only';
GRANT CONNECT ON DATABASE postgres TO fixture_reader;
GRANT USAGE ON SCHEMA public TO fixture_reader;
GRANT SELECT ON published_stories, news_status TO fixture_reader;
