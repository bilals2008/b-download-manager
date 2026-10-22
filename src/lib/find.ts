/**
 * Which row Find Next lands on, measured in the order the list shows rather
 * than by id. Wraps at both ends, and starts at the first match (or the last,
 * going backwards) when nothing has been picked yet.
 */
export function nextMatch(
  ids: number[],
  current: number | null,
  step: 1 | -1,
): number | null {
  if (ids.length === 0) {
    return null;
  }
  const at = current === null ? -1 : ids.indexOf(current);
  if (at === -1) {
    return step > 0 ? ids[0] : ids[ids.length - 1];
  }
  return ids[(at + step + ids.length) % ids.length];
}
