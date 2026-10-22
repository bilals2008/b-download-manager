/**
 * The ids a Shift click picks, measured in the order the list shows rather
 * than by id. Called with the visible ids, so a filter or a sort cannot make
 * a range jump over rows the user cannot see.
 */
export function rangeIds(
  ids: number[],
  anchor: number | null,
  target: number,
): number[] {
  const from = anchor === null ? -1 : ids.indexOf(anchor);
  const to = ids.indexOf(target);
  if (from < 0 || to < 0) {
    return [target];
  }
  const [start, end] = from <= to ? [from, to] : [to, from];
  return ids.slice(start, end + 1);
}
