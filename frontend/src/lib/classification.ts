// Transitional build snapshot of config/taxonomy.json; check_foundation enforces drift.
// Legacy rows have no geographic classification: language is never substituted.
import policy from "./taxonomy.generated.json" with { type: "json" };

export const sections = [...policy.sections, policy.fallback];

export function classify(topics: string[], geography: string[]) {
  const project = (values: string[], tags: typeof policy.topics) =>
    [...new Set(values.flatMap((value) => {
      const normalized = value.trim().toLowerCase();
      const tag = tags.find((tag) => tag.id === normalized || tag.aliases.includes(normalized));
      return tag ? [tag.id] : [];
    }))].sort();
  const mappedTopics = project(topics, policy.topics);
  const mappedGeography = project(geography, policy.geography);
  const section = policy.sections.find((section) =>
    (!section.any_topics.length || section.any_topics.some((topic) => mappedTopics.includes(topic))) &&
    (!section.any_geography.length || section.any_geography.some((place) => mappedGeography.includes(place)))) ?? policy.fallback;
  return { topics: mappedTopics, geography: mappedGeography, section };
}
export function legacySection(topic: string) {
  return classify(topic ? [topic] : [], []).section;
}
