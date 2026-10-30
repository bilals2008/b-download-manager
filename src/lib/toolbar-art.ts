/**
 * A drawing of a built-in toolbar icon as a data source for an `<img>`.
 *
 * Built-in drawings are written in this repository, so they are trusted and
 * skip the pack sanitizer; installed packs still go through it.
 */
export function toolbarIconDataUrl(
  body: string,
  defs = "",
  viewBox = "0 0 32 32",
): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}">${defs}${body}</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
