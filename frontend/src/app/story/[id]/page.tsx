// A single story — the "deep dive" page: French synthesis by default (toggle to the original),
// where sources disagree, links out to each outlet, and a shortcut to Jarvis. Reads the site's DB.
import {
  getStory, getThread, jarvisAskUrl, resolveTheme, rootVars, dispTitle, dispBody, frDate,
} from "../../../lib/news";
import StoryReader from "./StoryReader";

export const dynamic = "force-dynamic";

const CSS = `
@import url('https://fonts.googleapis.com/css2?family=UnifrakturMaguntia&display=swap');
*{box-sizing:border-box;}
.paper{min-height:100vh;background:var(--paper);color:var(--ink);font-family:Georgia,'Times New Roman',serif;}
.wrap{max-width:640px;margin:0 auto;padding:calc(26px*var(--s)) 26px calc(60px*var(--s));}
a{color:inherit;}
.back{font-size:11px;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);text-decoration:none;}
.kicker{font-size:10.5px;letter-spacing:.14em;text-transform:uppercase;color:var(--accent);margin:18px 0 8px;}
h1{font-size:clamp(28px,6vw,38px);line-height:1.08;font-weight:700;margin:0 0 12px;text-wrap:balance;}
.byline{font-size:12px;color:var(--muted);border-top:1px solid var(--hair);border-bottom:1px solid var(--hair);padding:10px 0;margin-bottom:14px;display:flex;justify-content:space-between;align-items:center;gap:12px;flex-wrap:wrap;}
.askbtn{display:inline-block;background:var(--accent);color:#fff;border-radius:3px;padding:8px 14px;font-size:12.5px;font-weight:700;text-decoration:none;}
.askbtn:hover{opacity:.9;}
.toggle{display:inline-block;background:none;border:1px solid var(--rule);color:var(--muted);border-radius:3px;padding:5px 11px;font-size:11.5px;font-family:inherit;cursor:pointer;margin-bottom:18px;}
.toggle:hover{border-color:var(--accent);color:var(--accent);}
.body{font-size:16px;line-height:1.72;text-align:justify;hyphens:auto;}
.body::first-letter{float:left;font-size:58px;line-height:.74;font-weight:700;padding:6px 9px 0 0;}
.summary{font-size:19px;line-height:1.55;font-style:italic;}
.pending{font-size:13px;color:var(--accent);margin-top:14px;}
.panel{border:1px solid var(--rule);border-radius:4px;padding:14px 16px;margin:24px 0;}
.panel h3{font-size:11px;letter-spacing:.1em;text-transform:uppercase;margin:0 0 10px;color:var(--accent);}
.dis{font-size:13.5px;line-height:1.5;margin-bottom:10px;}
.dis b{display:block;}
.dis span{color:var(--muted);}
.sources{margin:24px 0 0;}
.sources h3{font-size:11px;letter-spacing:.1em;text-transform:uppercase;margin:0 0 8px;color:var(--muted);border-bottom:1px solid var(--hair);padding-bottom:5px;}
.sources ul{list-style:none;padding:0;margin:0;display:flex;flex-wrap:wrap;gap:6px 18px;}
.sources a{font-size:13px;color:var(--accent);text-decoration:none;}
.sources a:hover{text-decoration:underline;}
.thread{margin:26px 0 0;}
.thread h3{font-size:11px;letter-spacing:.1em;text-transform:uppercase;margin:0 0 14px;color:var(--accent);border-bottom:1px solid var(--hair);padding-bottom:5px;}
.tl{list-style:none;margin:0;padding:0 0 0 4px;border-left:2px solid var(--rule);}
.tl li{position:relative;padding:0 0 16px 18px;}
.tl li::before{content:"";position:absolute;left:-6px;top:4px;width:9px;height:9px;border-radius:50%;background:var(--paper);border:2px solid var(--rule);}
.tl li.cur::before{border-color:var(--accent);background:var(--accent);}
.tl .d{font-size:10px;letter-spacing:.07em;text-transform:uppercase;color:var(--muted);margin-bottom:2px;}
.tl .h{font-size:14px;line-height:1.32;}
.tl li.cur .h{font-weight:700;}
.tl a{text-decoration:none;}
.tl a:hover{color:var(--accent);}
.colophon{text-align:center;border-top:3px double var(--ink);margin-top:30px;padding-top:14px;font-size:12px;color:var(--muted);font-style:italic;}
`;

function dayLabel(date: string) {
  return frDate(date).replace(/\s\d{4}$/, ""); // "lundi 8 juin" (year shown elsewhere)
}

export default async function StoryPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>;
  searchParams?: Promise<Record<string, string | undefined>>;
}) {
  const { id } = await params;
  const t = resolveTheme((searchParams ? await searchParams : {}) || {});
  const s = await getStory(id);
  const thread = s ? await getThread(s) : [];

  return (
    <main className="paper">
      <style dangerouslySetInnerHTML={{ __html: rootVars(t) + CSS }} />
      <div className="wrap">
        <a className="back" href={s?.edition_date ? `/edition/${s.edition_date}` : "/"}>‹ Retour à l’édition</a>
        {!s && <p style={{ marginTop: 30, color: t.p.muted }}>Cet article ne fait plus partie de l’édition en cours.</p>}
        {s && (
          <>
            <div className="kicker">{s.topic || (s.lang !== "en" ? "Belgique" : "Monde")}</div>
            <StoryReader
              titleFr={dispTitle(s)}
              bodyFr={dispBody(s)}
              titleOrig={s.title}
              bodyOrig={s.body}
              lang={s.lang}
              synthesized={s.synthesized}
              originCount={s.origin_count}
              sourceCount={s.source_count}
              sources={s.sources}
              disagreements={s.disagreements_json}
              disagreementsFr={s.disagreements_fr}
              askUrl={jarvisAskUrl(dispTitle(s))}
            />

            {thread.length > 1 && (
              <div className="thread">
                <h3>Suivi du sujet</h3>
                <ul className="tl">
                  {thread.map((e) => (
                    <li key={e.id} className={e.current ? "cur" : ""}>
                      <div className="d">{dayLabel(e.edition_date)} · ◆ {e.origin_count}</div>
                      <div className="h">
                        {e.current ? e.title : <a href={`/story/${e.id}`}>{e.title}</a>}
                      </div>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            <div className="colophon">
              {s.synthesized
                ? `Synthèse réalisée par Jarvis à partir de ${s.origin_count} ${s.origin_count === 1 ? "source" : "sources"}.`
                : `Résumé réalisé par Jarvis à partir de ${s.source_count} ${s.source_count === 1 ? "dépêche" : "dépêches"}.`}
              {" "}Pour aller plus loin — contexte, historique, enjeux — demandez à Jarvis.
            </div>
          </>
        )}
      </div>
    </main>
  );
}
