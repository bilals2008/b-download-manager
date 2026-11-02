import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { SETTINGS_PAGES } from "@/components/settings/settings-nav";
import {
  SETTINGS_SEARCH_ENTRIES,
  groupSettingsCatalogHits,
  searchSettingsCatalog,
} from "@/components/settings/settings-search-data";

const SETTINGS_DIR = join(process.cwd(), "src", "components", "settings");

function readSettingsSource(): string {
  const pages = readdirSync(SETTINGS_DIR)
    .filter((file) => file.endsWith("-settings.tsx"))
    .map((file) => readFileSync(join(SETTINGS_DIR, file), "utf8"));
  const shortcuts = readFileSync(
    join(process.cwd(), "src", "lib", "shortcuts.ts"),
    "utf8",
  );
  return [...pages, shortcuts].join("\n");
}

describe("settings search catalogue", () => {
  it("has a unique id and a real page for every entry", () => {
    const ids = new Set<string>();
    const pages = new Set(SETTINGS_PAGES.map((page) => page.id));
    for (const entry of SETTINGS_SEARCH_ENTRIES) {
      expect(entry.id, "an entry has no id").toBeTruthy();
      expect(ids.has(entry.id), `duplicate id ${entry.id}`).toBe(false);
      ids.add(entry.id);
      expect(pages.has(entry.page), `${entry.id} points at ${entry.page}`).toBe(
        true,
      );
      expect(entry.title.trim()).not.toBe("");
      expect(entry.keywords.length).toBeGreaterThan(0);
    }
  });

  it("gives every entry an element to scroll to", () => {
    const source = readSettingsSource();
    const missing = SETTINGS_SEARCH_ENTRIES.filter(
      (entry) =>
        !source.includes(`settingId="${entry.id}"`) &&
        !source.includes(`id="${entry.id}"`) &&
        !source.includes(`id: "${entry.id}"`),
    ).map((entry) => entry.id);
    expect(missing).toEqual([]);
  });

  it("answers common names and synonyms", () => {
    const cases: [string, string][] = [
      ["theme", "setting-theme"],
      ["dark mode", "setting-tray-dark-mode"],
      ["clipboard", "setting-clipboard"],
      ["autostart", "setting-launch-startup"],
      ["throttle", "setting-speed-limit-toggle"],
      ["bandwidth", "setting-speed-limit"],
      ["folder", "setting-default-folder"],
      ["temporary", "setting-unfinished-folder"],
      ["notification", "setting-complete-dialog"],
      ["storage", "setting-default-folder"],
      ["browser", "setting-browser-capture"],
      ["mp4", "setting-capture-types"],
      ["proxy", "setting-proxy"],
      ["defaults", "setting-reset"],
      ["update", "setting-auto-update"],
      ["ctrl n", "shortcut-add-download"],
      ["f3", "shortcut-find-next"],
      ["help", "shortcut-help"],
    ];

    for (const [query, id] of cases) {
      const ids = searchSettingsCatalog(query).map((hit) => hit.entry.id);
      expect(ids, `searching ${query}`).toContain(id);
    }
  });

  it("wins with the name before a keyword or a page", () => {
    const hits = searchSettingsCatalog("theme");
    expect(hits[0].entry.id).toBe("setting-theme");

    const speed = searchSettingsCatalog("speed limit");
    expect(speed[0].entry.id).toBe("setting-speed-limit");
  });

  it("groups results by page and numbers them in screen order", () => {
    const groups = groupSettingsCatalogHits(searchSettingsCatalog("folder"));
    expect(groups.length).toBeGreaterThan(0);
    expect(groups.map((group) => group.page)).toContain("save-locations");

    const indices = groups.flatMap((group) =>
      group.hits.map((hit) => hit.index),
    );
    expect(indices).toEqual(indices.map((_, index) => index));
  });

  it("returns nothing for an empty query and for gibberish", () => {
    expect(searchSettingsCatalog(" ")).toEqual([]);
    expect(searchSettingsCatalog("zzzzz")).toEqual([]);
  });

  it("stays quick over the real catalogue", () => {
    const started = performance.now();
    for (let run = 0; run < 1000; run += 1) {
      searchSettingsCatalog("speed");
    }
    expect(performance.now() - started).toBeLessThan(500);
  });
});
