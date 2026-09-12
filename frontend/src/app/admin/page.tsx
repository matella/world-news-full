// Admin dashboard — visibility into the news pipeline: what's been scraped, pooled, synthesised and
// published. Reads the site's own DB (status snapshot written by Jarvis + the published stories).
import { getStatus, getAdminStories } from "../../lib/news";

export const dynamic = "force-dynamic";

const CSS = `
*{box-sizing:border-box;}
.adm{min-height:100vh;background:#0e0f12;color:#e8e6e1;font-family:-apple-system,Segoe UI,Roboto,sans-serif;}
.wrap{max-width:920px;margin:0 auto;padding:30px 24px 60px;}
.top{display:flex;justify-content:space-between;align-items:baseline;border-bottom:1px solid #23262d;padding-bottom:14px;margin-bottom:22px;}
.top h1{font-size:22px;margin:0;font-weight:700;}
.top a{color:#7fb8a6;font-size:13px;text-decoration:none;}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:12px;margin-bottom:8px;}
.card{background:#16181d;border:1px solid #23262d;border-radius:8px;padding:16px;}
.card .v{font-size:30px;font-weight:700;line-height:1;}
.card .l{font-size:11px;letter-spacing:.08em;text-transform:uppercase;color:#8a8f98;margin-top:8px;}
.card.warn .v{color:#d6a05b;}
.card.good .v{color:#7fb8a6;}
.meta{font-size:12px;color:#8a8f98;margin:16px 0 26px;display:flex;gap:18px;flex-wrap:wrap;}
.meta b{color:#cfcdc7;font-weight:600;}
h2{font-size:13px;letter-spacing:.1em;text-transform:uppercase;color:#8a8f98;border-bottom:1px solid #23262d;padding-bottom:6px;margin:0 0 6px;}
table{width:100%;border-collapse:collapse;font-size:13px;}
th{text-align:left;color:#8a8f98;font-weight:600;font-size:11px;letter-spacing:.06em;text-transform:uppercase;padding:8px 10px;border-bottom:1px solid #23262d;}
td{padding:8px 10px;border-bottom:1px solid #1b1f26;vertical-align:top;}
td a{color:#e8e6e1;text-decoration:none;}
td a:hover{color:#7fb8a6;}
.chip{font-size:10.5px;text-transform:uppercase;color:#a98fd6;background:#1c1726;padding:2px 7px;border-radius:4px;}
.stat{font-size:10.5px;font-weight:600;padding:2px 7px;border-radius:4px;white-space:nowrap;}
.stat.full{color:#7fb8a6;background:#16241f;}
.stat.pend{color:#d6a05b;background:#2a2014;}
.stat.sum{color:#8a8f98;background:#1b1f26;}
.src{color:#7fb8a6;font-weight:600;}
.banner{background:#3a1f1a;border:1px solid #5c2e26;color:#e8b9a8;border-radius:8px;padding:12px 16px;margin-bottom:20px;font-size:13px;}
`;

function ago(iso?: string | null) {
  if (!iso) return "—";
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const h = Math.round(mins / 60);
  return h < 48 ? `${h} h ago` : `${Math.round(h / 24)} d ago`;
}

export default async function Admin() {
  const status = await getStatus();
  const stories = await getAdminStories();
  const s = status?.stats ?? {};
  const down = !status && stories.length === 0;

  return (
    <main className="adm">
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <div className="wrap">
        <div className="top">
          <h1>World News · pipeline admin</h1>
          <a href="/">‹ Back to the edition</a>
        </div>

        {down && (
          <div className="banner">
            ⚠ No data and no status snapshot — the database or the Jarvis publisher may be down.
            (Disk full? Check the box.)
          </div>
        )}

        <h2>Ingest &amp; pooling</h2>
        <div className="cards">
          <div className="card"><div className="v">{s.articles ?? "—"}</div><div className="l">Articles scraped</div></div>
          <div className={`card ${(s.pending ?? 0) > 0 ? "warn" : "good"}`}><div className="v">{s.pending ?? "—"}</div><div className="l">Processing queue{(s.process_eta_min ?? 0) > 0 ? ` · ~${s.process_eta_min}m` : ""}</div></div>
          <div className="card"><div className="v">{s.stories ?? "—"}</div><div className="l">Stories pooled</div></div>
          <div className="card good"><div className="v">{s.multi_source ?? "—"}</div><div className="l">Multi-source</div></div>
        </div>

        <h2 style={{ marginTop: 24 }}>Synthesis</h2>
        <div className="cards">
          <div className="card good"><div className="v">{s.synthesized ?? "—"}</div><div className="l">Full write-ups done</div></div>
          <div className={`card ${(s.awaiting_synthesis ?? 0) > 0 ? "warn" : "good"}`}><div className="v">{s.awaiting_synthesis ?? "—"}</div><div className="l">Awaiting synthesis{(s.synth_eta_min ?? 0) > 0 ? ` · ~${s.synth_eta_min}m` : ""}</div></div>
          <div className="card"><div className="v">{stories.length}</div><div className="l">Published to site</div></div>
        </div>

        <div className="meta">
          <span>Last ingest: <b>{ago(s.last_ingest)}</b></span>
          <span>Last publish: <b>{ago(status?.updated_at)}</b></span>
          <span>By language: <b>{s.by_lang ? Object.entries(s.by_lang).map(([k, v]) => `${k.toUpperCase()} ${v}`).join(" · ") : "—"}</b></span>
          <span>Translated → {(s.default_lang ?? "fr").toUpperCase()}: <b>{s.translated ?? "—"}</b>{(s.awaiting_translation ?? 0) > 0 ? ` · ${s.awaiting_translation} pending` : ""}</span>
        </div>

        <h2>Published stories ({stories.length})</h2>
        <table>
          <thead>
            <tr><th>Headline</th><th>Status</th><th>Section</th><th>Lang</th><th>Sources</th></tr>
          </thead>
          <tbody>
            {stories.map((st) => (
              <tr key={st.id}>
                <td><a href={`/story/${st.id}`}>{st.title}</a></td>
                <td>{st.synthesized
                  ? <span className="stat full">✓ Full</span>
                  : st.source_count > 1
                    ? <span className="stat pend">◐ Synthesising</span>
                    : <span className="stat sum">◦ Summary</span>}</td>
                <td>{st.topic ? <span className="chip">{st.topic}</span> : <span style={{ color: "#5c5f66" }}>—</span>}</td>
                <td style={{ color: "#8a8f98" }}>{st.lang.toUpperCase()}</td>
                <td><span className="src">◆ {st.origin_count}</span>{st.source_count > st.origin_count ? <span style={{ color: "#5c5f66" }}> /{st.source_count}</span> : null}</td>
              </tr>
            ))}
            {stories.length === 0 && (
              <tr><td colSpan={5} style={{ color: "#8a8f98", padding: "20px 10px" }}>No stories published yet.</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </main>
  );
}
