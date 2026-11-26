import { beforeEach, describe, expect, it, vi } from "vitest";

const STORAGE_KEY = "idm-clone.torrent-categories.v1";
const HASH = "a".repeat(40);

/** A fresh copy of the module, reading whatever storage holds now. */
async function loadModule() {
  vi.resetModules();
  return import("./torrent-categories");
}

describe("torrent categories", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("keeps the category the reader picked, across a restart", async () => {
    const first = await loadModule();
    expect(first.torrentCategoryOf(HASH)).toBeNull();

    first.rememberTorrentCategory(HASH, "Games");

    const restarted = await loadModule();
    expect(restarted.torrentCategoryOf(HASH)).toBe("Games");
  });

  it("follows a rename and a removal of the category", async () => {
    const module = await loadModule();
    module.rememberTorrentCategory(HASH, "Shows");

    module.renameTorrentCategory("Shows", "Series");
    expect(module.torrentCategoryOf(HASH)).toBe("Series");

    module.releaseTorrentCategory("Series");
    expect(module.torrentCategoryOf(HASH)).toBeNull();

    const restarted = await loadModule();
    expect(restarted.torrentCategoryOf(HASH)).toBeNull();
  });

  it("forgets a torrent that left the list", async () => {
    const module = await loadModule();
    module.rememberTorrentCategory(HASH, "Games");
    module.forgetTorrentCategory(HASH);

    expect(module.torrentCategoryOf(HASH)).toBeNull();
    expect(window.localStorage.getItem(STORAGE_KEY)).not.toContain(HASH);
  });

  it("ignores a saved file that is not a list of names", async () => {
    window.localStorage.setItem(STORAGE_KEY, "{not json");
    const broken = await loadModule();
    expect(broken.torrentCategoryOf(HASH)).toBeNull();

    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ version: 1, torrents: { [HASH]: 7 } }),
    );
    const wrongType = await loadModule();
    expect(wrongType.torrentCategoryOf(HASH)).toBeNull();
  });
});
