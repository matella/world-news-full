import Link from "next/link";
// Transitional, protected read-only archive overview. This is not worker health.
import { getAdminStories } from "../../lib/news";

export const dynamic = "force-dynamic";

const CSS = `
*{box-sizing:border-box;}
.adm{min-height:100vh;background:#0e0f12;color:#e8e6e1;font-family:-apple-system,Segoe UI,Roboto,sans-serif;}
.wrap{max-width:920px;margin:0 auto;padding:30px 24px 60px;}
.top{display:flex;justify-content:space-between;align-items:baseline;border-bottom:1px solid #23262d;padding-bottom:14px;margin-bottom:22px;}
.top h1{font-size:22px;margin:0;font-weight:700;}
.top a{color:#7fb8a6;font-size:13px;text-decoration:none;}
h2{font-size:13px;letter-spacing:.1em;text-transform:uppercase;color:#8a8f98;border-bottom:1px solid #23262d;padding-bottom:6px;margin:0 0 6px;}
table{width:100%;border-collapse:collapse;font-size:13px;}
th{text-align:left;color:#8a8f98;font-weight:600;font-size:11px;letter-spacing:.06em;text-transform:uppercase;padding:8px 10px;border-bottom:1px solid #23262d;}
td{padding:8px 10px;border-bottom:1px solid #1b1f26;vertical-align:top;}
td a{color:#e8e6e1;text-decoration:none;}
td a:hover{color:#7fb8a6;}
.chip{font-size:10.5px;text-transform:uppercase;color:#a98fd6;background:#1c1726;padding:2px 7px;border-radius:4px;}
.stat{font-size:10.5px;font-weight:600;padding:2px 7px;border-radius:4px;white-space:nowrap;}
.stat.full{color:#7fb8a6;background:#16241f;}
.stat.sum{color:#8a8f98;background:#1b1f26;}
.src{color:#7fb8a6;font-weight:600;}
.banner{background:#3a1f1a;border:1px solid #5c2e26;color:#e8b9a8;border-radius:8px;padding:12px 16px;margin-bottom:20px;font-size:13px;}
`;

export default async function Admin() {
  const stories = await getAdminStories();

  return (
    <main className="adm">
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <div className="wrap">
        <div className="top">
          <h1>World News · publications enregistrées</h1>
          <Link href="/">‹ Retour au journal</Link>
        </div>

        <p className="banner">
          Vue de lecture du système historique : au plus 200 publications récentes.
          Aucun état de collecte, délai de rédaction ou fonctionnement de traitement
          ne peut être déduit de cette liste. L’administration indépendante est
          disponible dans l’application Rust lorsqu’elle est configurée.
        </p>
        <h2>Publications récentes affichées ({stories.length})</h2>
        <table>
          <thead>
            <tr><th>Titre</th><th>Format enregistré</th><th>Rubrique</th><th>Langue</th><th>Références déclarées</th></tr>
          </thead>
          <tbody>
            {stories.map((st) => (
              <tr key={st.id}>
                <td><a href={`/story/${st.id}`}>{st.title}</a></td>
                <td>{st.synthesized
                  ? <span className="stat full">Synthèse enregistrée</span>
                  : <span className="stat sum">Résumé enregistré</span>}</td>
                <td>{st.topic ? <span className="chip">{st.topic}</span> : <span style={{ color: "#5c5f66" }}>—</span>}</td>
                <td style={{ color: "#8a8f98" }}>{st.lang.toUpperCase()}</td>
                <td><span className="src">{st.source_count}</span></td>
              </tr>
            ))}
            {stories.length === 0 && (
              <tr><td colSpan={5} style={{ color: "#8a8f98", padding: "20px 10px" }}>Aucune publication enregistrée.</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </main>
  );
}
