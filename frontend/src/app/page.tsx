// Front page = the latest edition (today, live). Past days live at /edition/[date]; all of them
// are listed at /archive. Rendering is the shared <Edition> component.
import { latestEditionDate, getEdition, adjacentEditions, resolveTheme } from "../lib/news";
import Edition from "./Edition";

export const dynamic = "force-dynamic";

export default async function Home({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | undefined>>;
}) {
  const t = resolveTheme((searchParams ? await searchParams : {}) || {});
  const date = await latestEditionDate();
  const stories = date ? await getEdition(date) : [];
  const adj = date ? await adjacentEditions(date) : { prev: null, next: null };

  return (
    <Edition
      t={t}
      stories={stories}
      editionDate={date}
      prev={adj.prev}
      next={null}        /* the latest edition has no "Demain" */
      isLatest
    />
  );
}
