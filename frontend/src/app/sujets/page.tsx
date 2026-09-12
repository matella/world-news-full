// « Sujets suivis » — les fils qui courent sur plusieurs éditions (la timeline vue d'ensemble).
// Une tête de fil = l'article le plus récent d'un sujet dont le fil traverse plusieurs jours.
import { activeThreads, resolveTheme, rootVars, frDate } from "../../lib/news";

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
.lead-note{font-size:13px;color:var(--muted);font-style:italic;text-align:center;margin:0 0 20px;}
.fil{border-bottom:1px solid var(--hair);padding:15px 2px;display:flex;justify-content:space-between;gap:14px;align-items:baseline;}
.fil h2{font-size:17px;line-height:1.3;margin:0;font-weight:700;flex:1;}
.fil h2 a:hover{color:var(--accent);}
.fil .meta{font-size:10.5px;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);white-space:nowrap;}
.fil .days{color:var(--accent);font-weight:700;}
.empty{text-align:center;color:var(--muted);font-style:italic;padding:26px 0;}
`;

export default async function Sujets({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | undefined>>;
}) {
  const t = resolveTheme((searchParams ? await searchParams : {}) || {});
  const threads = await activeThreads();

  return (
    <main className="paper">
      <style dangerouslySetInnerHTML={{ __html: rootVars(t) + CSS }} />
      <div className="wrap">
        <header className="masthead">
          <div className="title">{t.masthead}</div>
          <div className="sub">Sujets suivis</div>
        </header>
        <a className="back" href="/">‹ Revenir à l’édition du jour</a>
        <p className="lead-note">
          Les histoires qui courent sur plusieurs éditions — ouvrez-en une pour suivre le fil jour par jour.
        </p>

        {threads.length === 0 && (
          <p className="empty">Pas encore de sujet multi-jours — ça viendra avec les éditions.</p>
        )}
        {threads.map((th) => (
          <div className="fil" key={th.id}>
            <h2><a href={`/story/${th.id}`}>{th.title}</a></h2>
            <span className="meta">
              <span className="days">{th.days} jours</span> · {frDate(th.edition_date).replace(/\s\d{4}$/, "")}
            </span>
          </div>
        ))}
      </div>
    </main>
  );
}
