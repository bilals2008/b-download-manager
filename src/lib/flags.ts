// The flags of the countries a download's server can be in.
//
// The pictures are lipis/flag-icons 7.6.0 (MIT, © Panayiotis Lipiridis and
// contributors), in their 4x3 form, as the `flag-icons` package ships them —
// the same set the reference product draws a server's flag from. The package's
// licence text is in `node_modules/flag-icons/LICENSE`.
const files = import.meta.glob("../../node_modules/flag-icons/flags/4x3/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

const byCode = new Map<string, string>();
for (const [path, url] of Object.entries(files)) {
  const name = path.slice(path.lastIndexOf("/") + 1, -".svg".length);
  byCode.set(name.toLowerCase(), url);
}

/**
 * The address of the flag of a country, or nothing.
 *
 * The code is an ISO 3166-1 alpha-2 code as the engine reports it (`IN`), in
 * either case. Anything that is not two letters has no flag, and a country the
 * set does not carry has none either.
 */
export function flagUrl(code: string | null | undefined): string | null {
  const key = code?.trim().toLowerCase();
  if (!key || key.length !== 2) {
    return null;
  }
  return byCode.get(key) ?? null;
}
