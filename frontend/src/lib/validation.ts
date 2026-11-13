export function validEditionDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value < '0001-01-01') return false;
  const date = new Date(`${value}T12:00:00Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

export function validStoryId(value: string): boolean {
  // Legacy ID syntax is unknown; allow a bounded opaque segment, not only UUIDs.
  return value.length > 0 && value.length <= 200 && !/[\s\x00-\x1f\x7f/\\]/.test(value);
}
