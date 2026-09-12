// Recherche plein-texte dans toutes les éditions (français). Indépendant de Jarvis : tsvector
// sur la DB du site — pas d'embeddings nécessaires.
import { searchStories, resolveTheme, rootVars, frDate, dispTitle, dispBody } from "../../lib/news";

export const dynamic = "force-dynamic";

const CSS = `
@import url('https://fonts.googleapis.com/css2?family=UnifrakturMaguntia&display=swap');
*{box-sizing:border-box;}
.paper{min-height:100vh;background:var(--paper);color:var(--ink);font-family:Georgia,'Times New Roman',serif;}
.wrap{max-width:720px;margin:0 auto;padding:calc(24px*var(--s)) 28px 60px;}
a{color:inherit;text-decoration:none;}
.masthead{text-align:center;border-bottom:3px double var(--ink);padding-bottom:12px;margin-bottom:10px;}
.title{font-family:var(--mast);font-size:clamp(30px,7vw,44px);font-weight:700;line-height:1;margin:0;}
.sub{font-size:11px;letter-spacing:.2em;text-transform:uppercase;color:var(--muted);margin-top:8px;}
.back{display:block;text-align:center;font-size:11.5px;letter-spacing:.06em;text-transform:uppercase;color:var(--accent);padding:10px 0 18px;}
.back:hover{text-decoration:underline;}
.searchbox{display:flex;gap:8px;margin-bottom:24px;}
.searchbox input{flex:1;background:transparent;border:1px solid var(--rule);border-radius:3px;padding:10px 14px;font-family:inherit;font-size:15px;color:var(--ink);}
.searchbox button{background:var(--accent);color:#fff;border:none;border-radius:3px;padding:10px 18px;font-family:inherit;font-size:13px;font-weight:700;cursor:pointer;}
.hit{border-bottom:1px solid var(--hair);padding:14px 2px;}
.hit .d{font-size:10px;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);margin-bottom:3px;}
.hit h2{font-size:17px;line-height:1.25;margin:0 0 4px;font-weight:700;}
.hit h2 a:hover{color:var(--accent);}
.hit p{font-size:13px;line-height:1.5;color:var(--muted);margin:0;}
.empty{text-align:center;color:var(--muted);font-style:italic;padding:26px 0;}
`;

function snippet(body: string, n = 160) {
  return body ? body.slice(0, n).trimEnd() + (body.length > n ? "…" : "") : "";
}

export default async function Recherche({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | undefined>>;
}) {
  const sp = (searchParams ? await searchParams : {}) || {};
  const t = resolveTheme(sp);
  const q = (sp.q || "").trim();
  const hits = q ? await searchStories(q) : [];

  return (
    <main className="paper">
      <style dangerouslySetInnerHTML={{ __html: rootVars(t) + CSS }} />
      <div className="wrap">
        <header className="masthead">
          <div className="title">{t.masthead}</div>
          <div className="sub">Recherche dans les archives</div>
        </header>
        <a className="back" href="/">‹ Revenir à l’édition du jour</a>

        <form className="searchbox" action="/recherche" method="get">
          <input name="q" defaultValue={q} placeholder="Rechercher un sujet, un lieu, un nom…" autoFocus />
          <button type="submit">Chercher</button>
        </form>

        {q && hits.length === 0 && <p className="empty">Rien trouvé pour « {q} ».</p>}
        {hits.map((s) => (
          <article className="hit" key={s.id}>
            <div className="d">{s.edition_date ? frDate(s.edition_date) : ""} · ◆ {s.origin_count}</div>
            <h2><a href={`/story/${s.id}`}>{dispTitle(s)}</a></h2>
            <p>{snippet(dispBody(s))}</p>
          </article>
        ))}
      </div>
    </main>
  );
}
