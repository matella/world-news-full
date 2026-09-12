// Shared data + theme for the World News site. Reads the site's OWN Postgres (published by Jarvis).
import { Pool } from "pg";

const pool = new Pool({
  connectionString:
    process.env.DATABASE_URL ||
    "postgresql://worldnews:worldnews@postgres:5432/worldnews",
});

export type Claim = { claim: string; sources: number[] };
export type Disagreement = { point: string; positions: string[] };
export type Source = { name: string; url: string };
export type Story = {
  id: string;
  title: string;       // original (source-language) title
  body: string;        // original body
  title_fr: string;    // French translation (falls back to original if not yet translated/already FR)
  body_fr: string;
  lang: string;        // source language
  topic: string;
  origin_count: number;
  source_count: number;
  synthesized: boolean;
  claims_json: Claim[];
  disagreements_json: Disagreement[];     // original (source-language)
  disagreements_fr: Disagreement[];       // French translation
  sources: Source[];   // links to each outlet's original article
  edition_date: string;                   // YYYY-MM-DD — the day's edition this story belongs to
  related_ids: string[];                  // similar stories across days — the "subject thread"
};

const COLS =
  "id, title, body, lang, COALESCE(title_fr,'') AS title_fr, COALESCE(body_fr,'') AS body_fr, " +
  "COALESCE(topic,'') AS topic, origin_count, source_count, " +
  "COALESCE(synthesized,false) AS synthesized, " +
  "COALESCE(claims_json,'[]'::jsonb) AS claims_json, " +
  "COALESCE(disagreements_json,'[]'::jsonb) AS disagreements_json, " +
  "COALESCE(disagreements_fr,'[]'::jsonb) AS disagreements_fr, " +
  "COALESCE(sources_json,'[]'::jsonb) AS sources, " +
  "to_char(edition_date,'YYYY-MM-DD') AS edition_date, " +
  "COALESCE(related_ids,'{}') AS related_ids";

// The default view is French — fall back to the original until a translation lands (or if the
// story is already French). `dispTitle`/`dispBody` are what the paper renders by default.
export function dispTitle(s: Story) {
  return s.title_fr || s.title;
}
export function dispBody(s: Story) {
  return s.body_fr || s.body;
}

export async function getStories(): Promise<Story[]> {
  try {
    const { rows } = await pool.query(
      `SELECT ${COLS} FROM published_stories ORDER BY origin_count DESC, updated_at DESC LIMIT 40`
    );
    return rows as Story[];
  } catch {
    return [];
  }
}

// ── Editions (day-by-day archive) ──
export async function latestEditionDate(): Promise<string | null> {
  try {
    const { rows } = await pool.query(
      "SELECT to_char(max(edition_date),'YYYY-MM-DD') AS d FROM published_stories"
    );
    return rows[0]?.d ?? null;
  } catch {
    return null;
  }
}

// One day's edition — most-covered first (the lead), then recency.
export async function getEdition(date: string): Promise<Story[]> {
  try {
    const { rows } = await pool.query(
      `SELECT ${COLS} FROM published_stories WHERE edition_date = $1 ` +
        "ORDER BY origin_count DESC, updated_at DESC",
      [date]
    );
    return rows as Story[];
  } catch {
    return [];
  }
}

// The adjacent NON-EMPTY editions (skips days with no edition), for ‹ Hier / Demain ›.
export async function adjacentEditions(
  date: string
): Promise<{ prev: string | null; next: string | null }> {
  try {
    const { rows } = await pool.query(
      "SELECT to_char(max(edition_date) FILTER (WHERE edition_date < $1),'YYYY-MM-DD') AS prev, " +
        "to_char(min(edition_date) FILTER (WHERE edition_date > $1),'YYYY-MM-DD') AS next " +
        "FROM published_stories",
      [date]
    );
    return { prev: rows[0]?.prev ?? null, next: rows[0]?.next ?? null };
  } catch {
    return { prev: null, next: null };
  }
}

// The "subject thread": the current story + its related stories, oldest→newest — how the subject
// evolved across editions. Empty when the story has no strong matches.
export type ThreadEntry = {
  id: string;
  title: string;
  edition_date: string;
  origin_count: number;
  current: boolean;
};

export async function getThread(story: Story): Promise<ThreadEntry[]> {
  const ids = story.related_ids || [];
  if (!ids.length) return [];
  try {
    const { rows } = await pool.query(
      "SELECT id, COALESCE(NULLIF(title_fr,''), title) AS title, " +
        "to_char(edition_date,'YYYY-MM-DD') AS edition_date, origin_count " +
        "FROM published_stories WHERE id = ANY($1)",
      [ids]
    );
    const entries: ThreadEntry[] = rows.map((r) => ({ ...r, current: false }) as ThreadEntry);
    entries.push({
      id: story.id, title: dispTitle(story), edition_date: story.edition_date,
      origin_count: story.origin_count, current: true,
    });
    entries.sort((a, b) => a.edition_date.localeCompare(b.edition_date));
    return entries;
  } catch {
    return [];
  }
}

// Full-text search over the archive (French config; falls back to plain ILIKE on no hits).
export async function searchStories(q: string): Promise<Story[]> {
  if (!q.trim()) return [];
  try {
    const { rows } = await pool.query(
      `SELECT ${COLS} FROM published_stories ` +
        "WHERE to_tsvector('french', coalesce(title_fr,'') || ' ' || coalesce(body_fr,'') || ' ' " +
        "|| title || ' ' || body) @@ plainto_tsquery('french', $1) " +
        "ORDER BY edition_date DESC NULLS LAST, origin_count DESC LIMIT 30",
      [q.trim()]
    );
    if (rows.length) return rows as Story[];
    const like = await pool.query(
      `SELECT ${COLS} FROM published_stories WHERE title_fr ILIKE $1 OR title ILIKE $1 ` +
        "ORDER BY edition_date DESC NULLS LAST LIMIT 30",
      ["%" + q.trim() + "%"]
    );
    return like.rows as Story[];
  } catch {
    return [];
  }
}

// Active multi-day threads ("sujets suivis"): recent stories whose related_ids span other days.
export type ThreadHead = { id: string; title: string; edition_date: string; days: number };
export async function activeThreads(limit = 12): Promise<ThreadHead[]> {
  try {
    const { rows } = await pool.query(
      "SELECT p.id, COALESCE(NULLIF(p.title_fr,''), p.title) AS title, " +
        "to_char(p.edition_date,'YYYY-MM-DD') AS edition_date, " +
        "(SELECT count(DISTINCT r.edition_date) FROM published_stories r " +
        " WHERE r.id = ANY(p.related_ids) OR r.id = p.id)::int AS days " +
        "FROM published_stories p " +
        "WHERE EXISTS (SELECT 1 FROM published_stories r WHERE r.id = ANY(p.related_ids) " +
        "              AND r.edition_date <> p.edition_date) " +
        "ORDER BY p.edition_date DESC, p.origin_count DESC LIMIT $1",
      [limit]
    );
    // garde une tête par fil : si deux têtes partagent des related, la plus récente gagne
    const seen = new Set<string>();
    const heads: ThreadHead[] = [];
    for (const r of rows as (ThreadHead & { id: string })[]) {
      if (seen.has(r.id)) continue;
      seen.add(r.id);
      heads.push(r);
    }
    return heads;
  } catch {
    return [];
  }
}

// Every edition (newest first) with its story count — the archive index.
export async function editionDates(): Promise<{ date: string; count: number }[]> {
  try {
    const { rows } = await pool.query(
      "SELECT to_char(edition_date,'YYYY-MM-DD') AS date, count(*)::int AS count " +
        "FROM published_stories WHERE edition_date IS NOT NULL " +
        "GROUP BY edition_date ORDER BY edition_date DESC"
    );
    return rows as { date: string; count: number }[];
  } catch {
    return [];
  }
}

export type Status = {
  stats: {
    articles?: number;
    pending?: number;
    stories?: number;
    multi_source?: number;
    synthesized?: number;
    awaiting_synthesis?: number;
    translated?: number;
    awaiting_translation?: number;
    default_lang?: string;
    process_eta_min?: number;
    synth_eta_min?: number;
    by_lang?: Record<string, number>;
    last_ingest?: string | null;
  };
  updated_at: string;
};

export async function getStatus(): Promise<Status | null> {
  try {
    const { rows } = await pool.query(
      "SELECT stats, updated_at FROM news_status WHERE id = 1"
    );
    return (rows[0] as Status) ?? null;
  } catch {
    return null;
  }
}

export async function getAdminStories(): Promise<Story[]> {
  try {
    const { rows } = await pool.query(
      `SELECT ${COLS} FROM published_stories ORDER BY updated_at DESC LIMIT 200`
    );
    return rows as Story[];
  } catch {
    return [];
  }
}

export async function getStory(id: string): Promise<Story | null> {
  try {
    const { rows } = await pool.query(
      `SELECT ${COLS} FROM published_stories WHERE id = $1`,
      [id]
    );
    return (rows[0] as Story) ?? null;
  } catch {
    return null;
  }
}

export function byline(s: Story) {
  const n = s.origin_count || 1;
  return `Synthèse · ${n} source${n === 1 ? "" : "s"}`;
}

// "lundi 8 juin 2026" from a YYYY-MM-DD edition date (noon avoids a TZ off-by-one).
export function frDate(date: string) {
  return new Intl.DateTimeFormat("fr-BE", {
    weekday: "long", day: "numeric", month: "long", year: "numeric",
  }).format(new Date(date + "T12:00:00"));
}

// A deep-link into the Jarvis console, pre-loaded to deep-dive this story.
export function jarvisAskUrl(title: string) {
  const base = process.env.NEXT_PUBLIC_JARVIS_URL || "http://192.168.129.85:8092";
  return `${base}/?ask=${encodeURIComponent(`Fais-moi une analyse approfondie de cette actualité : ${title}`)}`;
}

// ── Theme parameters ──
export type Palette = { paper: string; ink: string; rule: string; hair: string; muted: string; accent: string };
export const PAPERS: Record<string, Palette> = {
  cream: { paper: "#f4f1e9", ink: "#1a1814", rule: "#cfc7b5", hair: "#e3dccb", muted: "#5c574c", accent: "#9a3b2e" },
  crisp: { paper: "#fdfcf9", ink: "#14110d", rule: "#d8d2c4", hair: "#ece8df", muted: "#6a6458", accent: "#8a2f24" },
  sepia: { paper: "#efe6d2", ink: "#2a2418", rule: "#cdbfa3", hair: "#e0d5bd", muted: "#6b5f47", accent: "#8a4a2e" },
  ink: { paper: "#15140f", ink: "#ece8df", rule: "#3a362c", hair: "#2a271f", muted: "#9a9384", accent: "#d98a5b" },
};
export const DENSITY: Record<string, string> = { airy: "1.18", normal: "1", dense: "0.82" };
export const FONTS: Record<string, string> = {
  serif: "Georgia,'Times New Roman',serif",
  blackletter: "'UnifrakturMaguntia',Georgia,serif",
  slab: "'Rockwell','Roboto Slab',Georgia,serif",
};

export function resolveTheme(sp: Record<string, string | undefined>) {
  const pick = (v: string | undefined, env: string | undefined, def: string, keys: string[]) =>
    v && keys.includes(v) ? v : env && keys.includes(env) ? env : def;
  const p = PAPERS[pick(sp.paper, process.env.NEWS_PAPER, "cream", Object.keys(PAPERS))];
  const s = DENSITY[pick(sp.density, process.env.NEWS_DENSITY, "normal", Object.keys(DENSITY))];
  const mastFont = FONTS[pick(sp.font, process.env.NEWS_FONT, "serif", Object.keys(FONTS))];
  const masthead = sp.masthead || process.env.NEWS_MASTHEAD || "The World Brief";
  return { p, s, mastFont, masthead };
}

export function rootVars(t: ReturnType<typeof resolveTheme>) {
  return (
    `:root{--paper:${t.p.paper};--ink:${t.p.ink};--rule:${t.p.rule};--hair:${t.p.hair};` +
    `--muted:${t.p.muted};--accent:${t.p.accent};--s:${t.s};--mast:${t.mastFont};}`
  );
}
