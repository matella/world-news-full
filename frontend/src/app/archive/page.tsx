// The archive index — every edition ever published, grouped by month, newest first. Jump to any day.
import { editionDates, resolveTheme, rootVars, frDate } from "../../lib/news";

export const dynamic = "force-dynamic";

const CSS = `
@import url('https://fonts.googleapis.com/css2?family=UnifrakturMaguntia&display=swap');
*{box-sizing:border-box;}
.paper{min-height:100vh;background:var(--paper);color:var(--ink);font-family:Georgia,'Times New Roman',serif;}
.wrap{max-width:720px;margin:0 auto;padding:calc(24px*var(--s)) 28px 60px;}
a{color:inherit;text-decoration:none;}
.masthead{text-align:center;border-bottom:3px double var(--ink);padding-bottom:12px;margin-bottom:8px;}
.title{font-family:var(--mast);font-size:clamp(30px,7vw,44px);font-weight:700;line-height:1;margin:0;}
.sub{font-size:11px;letter-spacing:.2em;text-transform:uppercase;color:var(--muted);margin-top:8px;}
.back{display:block;text-align:center;font-size:11.5px;letter-spacing:.06em;text-transform:uppercase;color:var(--accent);padding:10px 0 22px;}
.back:hover{text-decoration:underline;}
.month{font-size:12px;letter-spacing:.12em;text-transform:uppercase;color:var(--muted);border-bottom:2px solid var(--ink);padding-bottom:4px;margin:22px 0 4px;}
.row{display:flex;justify-content:space-between;align-items:baseline;border-bottom:1px solid var(--hair);padding:11px 2px;gap:12px;}
.row a{font-size:16px;text-transform:capitalize;}
.row a:hover{color:var(--accent);}
.count{font-size:11px;color:var(--muted);white-space:nowrap;}
.empty{text-align:center;color:var(--muted);font-style:italic;padding:30px 0;}
`;

function monthKey(date: string) {
  return new Intl.DateTimeFormat("fr-BE", { month: "long", year: "numeric" })
    .format(new Date(date + "T12:00:00"));
}
function dayLabel(date: string) {
  // "lundi 8 juin" — drop the year (shown in the month header)
  return frDate(date).replace(/\s\d{4}$/, "");
}

export default async function Archive({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | undefined>>;
}) {
  const t = resolveTheme((searchParams ? await searchParams : {}) || {});
  const eds = await editionDates();
  const groups: { month: string; days: { date: string; count: number }[] }[] = [];
  for (const e of eds) {
    const m = monthKey(e.date);
    const g = groups[groups.length - 1];
    if (g && g.month === m) g.days.push(e);
    else groups.push({ month: m, days: [e] });
  }

  return (
    <main className="paper">
      <style dangerouslySetInnerHTML={{ __html: rootVars(t) + CSS }} />
      <div className="wrap">
        <header className="masthead">
          <div className="title">{t.masthead}</div>
          <div className="sub">Toutes les éditions</div>
        </header>
        <a className="back" href="/">‹ Revenir à l’édition du jour</a>

        {eds.length === 0 && <p className="empty">Aucune édition archivée pour l’instant.</p>}

        {groups.map((g) => (
          <section key={g.month}>
            <div className="month">{g.month}</div>
            {g.days.map((d) => (
              <div className="row" key={d.date}>
                <a href={`/edition/${d.date}`}>{dayLabel(d.date)}</a>
                <span className="count">{d.count} article{d.count === 1 ? "" : "s"}</span>
              </div>
            ))}
          </section>
        ))}
      </div>
    </main>
  );
}
