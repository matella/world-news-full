// One newspaper edition (a single day). Rendered by both the front page (latest, live) and every
// archived day at /edition/[date]. Pure presentation — takes the day's stories + page-turn links.
import {
  byline, jarvisAskUrl, rootVars, frDate, dispTitle, dispBody, Story,
} from "../lib/news";

const CSS = `
@import url('https://fonts.googleapis.com/css2?family=UnifrakturMaguntia&display=swap');
*{box-sizing:border-box;}
.paper{min-height:100vh;background:var(--paper);color:var(--ink);font-family:Georgia,'Times New Roman',serif;}
.wrap{max-width:920px;margin:0 auto;padding:calc(24px*var(--s)) 28px calc(50px*var(--s));}
a{color:inherit;text-decoration:none;}
.dateline{display:flex;justify-content:space-between;font-size:10.5px;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);border-bottom:1px solid var(--rule);padding-bottom:6px;}
.masthead{text-align:center;border-bottom:3px double var(--ink);padding-bottom:calc(12px*var(--s));margin-bottom:calc(14px*var(--s));}
.title{font-family:var(--mast);font-size:clamp(38px,9vw,56px);font-weight:700;line-height:1;letter-spacing:-.01em;margin:10px 0 0;}
.motto{font-size:11px;letter-spacing:.2em;text-transform:uppercase;color:var(--muted);margin-top:10px;}
.pageturn{display:flex;justify-content:space-between;align-items:center;font-size:11.5px;letter-spacing:.06em;text-transform:uppercase;border-top:1px solid var(--hair);border-bottom:1px solid var(--hair);padding:8px 2px;margin-bottom:calc(20px*var(--s));}
.pageturn a{color:var(--accent);}
.pageturn a:hover{text-decoration:underline;}
.pageturn .off{color:var(--hair);}
.pageturn .arch{color:var(--muted);letter-spacing:.1em;}
.kicker{font-size:10.5px;letter-spacing:.12em;text-transform:uppercase;color:var(--accent);margin-bottom:6px;}
.lead{border-bottom:1px solid var(--rule);padding-bottom:calc(22px*var(--s));margin-bottom:calc(20px*var(--s));}
.lead h1{font-size:clamp(28px,5.2vw,38px);line-height:1.06;font-weight:700;margin:0 0 12px;text-wrap:balance;}
.lead h1 a:hover{color:var(--accent);}
.lead .body{font-size:14.5px;line-height:1.64;text-align:justify;hyphens:auto;column-gap:30px;column-count:2;}
.lead .body::first-letter{float:left;font-size:54px;line-height:.76;font-weight:700;padding:5px 9px 0 0;}
.byline{font-size:11px;color:var(--muted);border-top:1px solid var(--hair);padding-top:7px;margin-top:13px;display:flex;justify-content:space-between;gap:10px;flex-wrap:wrap;}
.ask{color:var(--accent);font-size:11px;white-space:nowrap;}
.ask:hover{text-decoration:underline;}
.sections{columns:3;column-gap:30px;column-rule:1px solid var(--rule);}
.section{break-inside:avoid;margin:0 0 22px;}
.sechead{font-size:10.5px;letter-spacing:.14em;text-transform:uppercase;border-bottom:2px solid var(--ink);padding-bottom:4px;margin-bottom:calc(12px*var(--s));}
.item + .item{border-top:1px solid var(--hair);padding-top:calc(13px*var(--s));margin-top:calc(13px*var(--s));}
.item h2{font-size:15.5px;line-height:1.22;font-weight:700;margin:0 0 5px;text-wrap:balance;}
.item h2 a:hover{color:var(--accent);}
.item .snip{font-size:12.5px;line-height:1.5;color:var(--muted);margin:0 0 6px;}
.item .meta{font-size:10.5px;color:var(--muted);display:flex;justify-content:space-between;gap:8px;}
.colophon{text-align:center;border-top:3px double var(--ink);margin-top:26px;padding-top:13px;font-size:11px;color:var(--muted);font-style:italic;}
@media(max-width:880px){.sections{columns:2;}}
@media(max-width:560px){.lead .body{column-count:1;}.sections{columns:1;}.dateline{flex-direction:column;gap:2px;align-items:center;}}
`;

function snippet(body: string, n = 150) {
  return body ? body.slice(0, n).trimEnd() + (body.length > n ? "…" : "") : "";
}

function sectionsOf(stories: Story[]) {
  const sport = stories.filter((s) => s.topic === "sport");
  const used = new Set(sport.map((s) => s.id));
  const take = (pred: (s: Story) => boolean) =>
    stories.filter((s) => !used.has(s.id) && pred(s)).map((s) => (used.add(s.id), s));
  const belgique = take((s) => s.lang !== "en");
  const techbiz = take((s) => ["tech", "finance", "science"].includes(s.topic));
  const monde = take(() => true);
  return [
    { title: "Monde", items: monde.slice(0, 6) },
    { title: "Belgique", items: belgique.slice(0, 6) },
    { title: "Tech & Éco", items: techbiz.slice(0, 5) },
    { title: "Sport", items: sport.slice(0, 4) },
  ].filter((s) => s.items.length);
}

function Item({ s }: { s: Story }) {
  const body = dispBody(s);
  return (
    <article className="item">
      <h2><a href={`/story/${s.id}`}>{dispTitle(s)}</a></h2>
      {body && <p className="snip">{snippet(body)}</p>}
      <div className="meta">
        <span>{byline(s)}</span>
        <a className="ask" href={jarvisAskUrl(dispTitle(s))} target="_blank" rel="noreferrer">Jarvis ↗</a>
      </div>
    </article>
  );
}

export default function Edition({
  t, stories, editionDate, prev, next, isLatest,
}: {
  t: ReturnType<typeof import("../lib/news").resolveTheme>;
  stories: Story[];
  editionDate: string | null;
  prev: string | null;
  next: string | null;
  isLatest: boolean;
}) {
  const lead = stories[0];
  const sections = sectionsOf(stories.slice(1));
  const dateLabel = editionDate ? frDate(editionDate) : "";

  return (
    <main className="paper">
      <style dangerouslySetInnerHTML={{ __html: rootVars(t) + CSS }} />
      <div className="wrap">
        <header className="masthead">
          <div className="dateline">
            <span>{dateLabel}</span>
            <span>{isLatest ? "Édition du jour" : "Édition archivée"}</span>
            <span>Liège</span>
          </div>
          <div className="title">{t.masthead}</div>
          <div className="motto">Synthèse de dix sources · monde &amp; Belgique · indépendant</div>
        </header>

        <nav className="pageturn">
          {prev ? <a href={`/edition/${prev}`}>‹ Hier</a> : <span className="off">‹ Hier</span>}
          <span className="arch">
            <a href="/sujets">Sujets suivis</a> · <a href="/archive">Archives</a> · <a href="/recherche">Rechercher</a>
          </span>
          {next ? <a href={`/edition/${next}`}>Demain ›</a> : <span className="off">Demain ›</span>}
        </nav>

        {!lead && <p style={{ textAlign: "center", color: t.p.muted }}>Pas d’édition pour cette journée.</p>}

        {lead && (
          <section className="lead">
            <div className="kicker">À la une</div>
            <h1><a href={`/story/${lead.id}`}>{dispTitle(lead)}</a></h1>
            {dispBody(lead) && <div className="body">{snippet(dispBody(lead), 540)}</div>}
            <div className="byline">
              <span>{byline(lead)} · ◆ {lead.origin_count}</span>
              <a className="ask" href={jarvisAskUrl(dispTitle(lead))} target="_blank" rel="noreferrer">Analyse complète par Jarvis ↗</a>
            </div>
          </section>
        )}

        <div className="sections">
          {sections.map((sec) => (
            <div className="section" key={sec.title}>
              <div className="sechead">{sec.title}</div>
              {sec.items.map((s) => <Item key={s.id} s={s} />)}
            </div>
          ))}
        </div>

        <div className="colophon">
          {isLatest
            ? "Voilà l’édition du jour. Touchez un article, ou demandez l’analyse à Jarvis."
            : "Édition archivée. Tournez la page, ou parcourez toutes les éditions."}
        </div>
      </div>
    </main>
  );
}
