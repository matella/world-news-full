'use client';

export default function ErrorPage({ reset }: { reset: () => void }) {
  return <main lang="fr"><h1>Le journal est temporairement indisponible.</h1><p>Les données n’ont pas pu être chargées. Réessayez dans un instant.</p><button onClick={reset}>Réessayer</button></main>;
}
