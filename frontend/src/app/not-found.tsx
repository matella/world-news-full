import Link from "next/link";
export default function NotFound() {
  return <main lang="fr"><h1>Page introuvable</h1><p>Cet article ou cette édition n’existe pas.</p><Link href="/">Revenir au journal</Link></main>;
}
