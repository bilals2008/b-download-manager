/**
 * The category of a torrent row, when the reader picked one.
 *
 * A torrent is not an engine download, so the engine index cannot hold its
 * category: the row is rebuilt from the torrent summaries every tick, and each
 * rebuild would guess the category from the torrent's name and drop the
 * reader's choice. The choice is kept here, by info hash, and saved beside the
 * other webview preferences. A torrent with no choice of its own keeps the
 * category its name suggests.
 */

import { MAX_CATEGORY_NAME, type CategoryName } from "@/lib/categories";

const STORAGE_KEY = "idm-clone.torrent-categories.v1";
const STORAGE_VERSION = 1;

/** What the reader chose, by info hash. */
const chosen = new Map<string, CategoryName>();

function usableName(value: unknown): CategoryName | null {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > MAX_CATEGORY_NAME) {
    return null;
  }
  return trimmed;
}

/** Read the saved choices. A bad file is no choices, never a failed start. */
function load(): Map<string, CategoryName> {
  const out = new Map<string, CategoryName>();
  try {
    if (typeof window === "undefined") {
      return out;
    }
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return out;
    }
    const parsed: unknown = JSON.parse(raw);
    const saved =
      parsed && typeof parsed === "object"
        ? (parsed as { torrents?: unknown }).torrents
        : null;
    if (!saved || typeof saved !== "object") {
      return out;
    }
    for (const [hash, value] of Object.entries(saved)) {
      const name = usableName(value);
      if (hash && name) {
        out.set(hash, name);
      }
    }
  } catch {
    // Storage can be blocked or hold junk; every torrent then names itself.
  }
  return out;
}

function save(): void {
  try {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        version: STORAGE_VERSION,
        torrents: Object.fromEntries(chosen),
      }),
    );
  } catch {
    // Storage can be blocked; the choices still work for this session.
  }
}

for (const [hash, name] of load()) {
  chosen.set(hash, name);
}

/** The category the reader gave a torrent, or null when they gave none. */
export function torrentCategoryOf(infoHash: string): CategoryName | null {
  return chosen.get(infoHash) ?? null;
}

/** Keep the category the reader picked for a torrent. */
export function rememberTorrentCategory(
  infoHash: string,
  category: CategoryName,
): void {
  if (chosen.get(infoHash) === category) {
    return;
  }
  chosen.set(infoHash, category);
  save();
}

/** The torrent left the list; its choice leaves with it. */
export function forgetTorrentCategory(infoHash: string): void {
  if (chosen.delete(infoHash)) {
    save();
  }
}

/** A category was renamed, so the torrents in it keep their place. */
export function renameTorrentCategory(from: CategoryName, to: CategoryName): void {
  let changed = false;
  for (const [hash, name] of chosen) {
    if (name === from) {
      chosen.set(hash, to);
      changed = true;
    }
  }
  if (changed) {
    save();
  }
}

/** A category was removed, so its torrents fall back to their own names. */
export function releaseTorrentCategory(name: CategoryName): void {
  let changed = false;
  for (const [hash, category] of chosen) {
    if (category === name) {
      chosen.delete(hash);
      changed = true;
    }
  }
  if (changed) {
    save();
  }
}
