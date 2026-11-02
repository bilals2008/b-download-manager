/**
 * The search model behind the Settings window.
 *
 * The catalogue of entries lives with the settings pages; this module only
 * prepares it once and answers queries against the prepared form. Nothing here
 * reads the DOM, waits on the engine or rebuilds per keystroke: the index is a
 * plain array of normalized strings, so a search is one pass over a few dozen
 * entries and stays clear of the render path.
 */

export interface SettingsSearchEntry {
  /** Unique, and the element the result scrolls to. */
  id: string;
  page: string;
  section: string;
  title: string;
  description: string;
  /** Synonyms and key names, not shown beside the title. */
  keywords: readonly string[];
}

export interface SettingsSearchIndexItem<
  Entry extends SettingsSearchEntry = SettingsSearchEntry,
> {
  entry: Entry;
  title: string;
  description: string;
  keywords: string;
  section: string;
  page: string;
  pageLabel: string;
}

export interface SettingsSearchHit<Entry extends SettingsSearchEntry> {
  entry: Entry;
  score: number;
}

export interface SettingsSearchGroup<
  Entry extends SettingsSearchEntry,
> {
  page: string;
  label: string;
  hits: (SettingsSearchHit<Entry> & { index: number })[];
}

/** Lower case without diacritics, so `resume` and `résumé` answer alike. */
export function normalizeSearchText(value: string): string {
  return value
    .toLocaleLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "");
}

/**
 * Split a query into words. A single-character word is only kept when it is
 * the whole query: `ctrl+n` should look for the key, not every `n` in the app.
 */
export function searchTokens(query: string): string[] {
  const tokens = normalizeSearchText(query)
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
  const words = tokens.filter((token) => token.length > 1);
  return words.length > 0 ? words : tokens;
}

/** Prepare the catalogue once; querying never touches the raw entries again. */
export function buildSettingsSearchIndex<Entry extends SettingsSearchEntry>(
  entries: readonly Entry[],
  pageLabels: ReadonlyMap<string, string>,
): SettingsSearchIndexItem<Entry>[] {
  return entries.map((entry) => ({
    entry,
    title: normalizeSearchText(entry.title),
    description: normalizeSearchText(entry.description),
    keywords: entry.keywords
      .flatMap((keyword) => {
        const normalized = normalizeSearchText(keyword);
        return [normalized, normalized.replace(/[^a-z0-9]+/g, " ")];
      })
      .join(" "),
    section: normalizeSearchText(entry.section),
    page: normalizeSearchText(entry.page),
    pageLabel: normalizeSearchText(pageLabels.get(entry.page) ?? entry.page),
  }));
}

/**
 * How well one word matches one entry. The name wins over a synonym, a
 * synonym over the hint, and the page name only breaks ties: searching `tray`
 * should open the tray switch, not every row on a page whose name contains it.
 */
function tokenScore(
  item: SettingsSearchIndexItem,
  token: string,
): number {
  if (item.title === token) return 120;
  if (item.title.startsWith(token)) return 90;
  if (item.title.includes(token)) return 70;
  if (item.keywords.includes(token)) return 50;
  if (item.description.includes(token)) return 25;
  if (item.section.includes(token) || item.pageLabel.includes(token)) {
    return 12;
  }
  return 0;
}

/**
 * Find the settings that match every word of the query, best first.
 *
 * The result is capped so a very broad word (`a`) cannot grow the list panel
 * without bound, and the cap is applied after scoring so the best hits survive.
 */
export function searchSettings<Entry extends SettingsSearchEntry>(
  query: string,
  index: readonly SettingsSearchIndexItem<Entry>[],
  limit = 40,
): SettingsSearchHit<Entry>[] {
  const tokens = searchTokens(query);
  if (tokens.length === 0) {
    return [];
  }

  const phrase = tokens.join(" ");
  const hits: SettingsSearchHit<Entry>[] = [];

  for (const item of index) {
    let score = 0;
    let matched = true;
    for (const token of tokens) {
      const value = tokenScore(item, token);
      if (value === 0) {
        matched = false;
        break;
      }
      score += value;
    }
    if (!matched) {
      continue;
    }
    if (item.title.includes(phrase)) {
      score += 40;
    } else if (item.keywords.includes(phrase)) {
      score += 25;
    }
    hits.push({ entry: item.entry, score });
  }

  hits.sort(
    (left, right) =>
      right.score - left.score ||
      left.entry.title.localeCompare(right.entry.title),
  );

  return limit > 0 ? hits.slice(0, limit) : hits;
}

/**
 * Group hits under their page, keeping the order of the best hit in each page
 * and numbering the rows after grouping so keyboard arrows follow what is on
 * screen.
 */
export function groupSettingsHits<Entry extends SettingsSearchEntry>(
  hits: readonly SettingsSearchHit<Entry>[],
  pageLabels: ReadonlyMap<string, string>,
  pageOrder?: readonly string[],
): SettingsSearchGroup<Entry>[] {
  const byPage = new Map<string, SettingsSearchGroup<Entry>>();
  for (const hit of hits) {
    let group = byPage.get(hit.entry.page);
    if (!group) {
      group = {
        page: hit.entry.page,
        label: pageLabels.get(hit.entry.page) ?? hit.entry.page,
        hits: [],
      };
      byPage.set(hit.entry.page, group);
    }
    group.hits.push({ ...hit, index: 0 });
  }

  const groups = [...byPage.values()];
  if (pageOrder) {
    const order = new Map(pageOrder.map((page, position) => [page, position]));
    groups.sort(
      (left, right) =>
        (order.get(left.page) ?? Number.MAX_SAFE_INTEGER) -
        (order.get(right.page) ?? Number.MAX_SAFE_INTEGER),
    );
  }

  let index = 0;
  for (const group of groups) {
    group.hits = group.hits.map((hit) => ({ ...hit, index: index++ }));
  }

  return groups;
}
