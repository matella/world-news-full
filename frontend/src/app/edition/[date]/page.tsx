// A single archived day's edition. Same <Edition> rendering as the front page, with ‹ Hier / Demain ›
// to the adjacent non-empty days.
import { getEdition, adjacentEditions, latestEditionDate, resolveTheme } from "../../../lib/news";
import Edition from "../../Edition";

export const dynamic = "force-dynamic";

export default async function EditionPage({
  params,
  searchParams,
}: {
  params: Promise<{ date: string }>;
  searchParams?: Promise<Record<string, string | undefined>>;
}) {
  const { date } = await params;
  const t = resolveTheme((searchParams ? await searchParams : {}) || {});
  const [stories, adj, latest] = await Promise.all([
    getEdition(date),
    adjacentEditions(date),
    latestEditionDate(),
  ]);
  const isLatest = date === latest;

  return (
    <Edition
      t={t}
      stories={stories}
      editionDate={date}
      prev={adj.prev}
      next={isLatest ? null : adj.next}
      isLatest={isLatest}
    />
  );
}
