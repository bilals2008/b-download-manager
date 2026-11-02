import { describe, expect, it } from "vitest";

import {
  buildSettingsSearchIndex,
  groupSettingsHits,
  normalizeSearchText,
  searchSettings,
  searchTokens,
  type SettingsSearchEntry,
} from "@/lib/settings-search";

const LABELS = new Map([
  ["general", "General"],
  ["connections", "Connections"],
]);

const ENTRIES: SettingsSearchEntry[] = [
  {
    id: "startup",
    page: "general",
    section: "Startup",
    title: "Launch on startup",
    description: "Adds an entry to the startup of this computer.",
    keywords: ["autostart", "boot", "login"],
  },
  {
    id: "speed",
    page: "connections",
    section: "Speed limit",
    title: "Speed limit",
    description: "Kilobytes per second.",
    keywords: ["bandwidth", "throttle", "cap"],
  },
  {
    id: "speed-toggle",
    page: "connections",
    section: "Speed limit",
    title: "Limit the speed of all downloads",
    description: "Shared by all downloads.",
    keywords: ["bandwidth", "throttle"],
  },
  {
    id: "tray",
    page: "general",
    section: "The tray",
    title: "Show the tray icon",
    description: "The notification area icon.",
    keywords: ["system tray"],
  },
];

const INDEX = buildSettingsSearchIndex(ENTRIES, LABELS);

describe("normalizeSearchText", () => {
  it("folds case and accents", () => {
    expect(normalizeSearchText("Résumé")).toBe("resume");
    expect(normalizeSearchText("SPEED Limit")).toBe("speed limit");
  });
});

describe("searchTokens", () => {
  it("splits on punctuation and keeps words", () => {
    expect(searchTokens("ctrl+n")).toEqual(["ctrl"]);
    expect(searchTokens("speed-limit")).toEqual(["speed", "limit"]);
  });

  it("keeps a lone single-character query", () => {
    expect(searchTokens("a")).toEqual(["a"]);
  });
});

describe("searchSettings", () => {
  it("finds a setting by its name", () => {
    const hits = searchSettings("speed limit", INDEX);
    expect(hits[0].entry.id).toBe("speed");
  });

  it("finds a setting by a synonym", () => {
    const hits = searchSettings("throttle", INDEX);
    expect(hits.map((hit) => hit.entry.id)).toContain("speed-toggle");
  });

  it("finds a setting by its description", () => {
    const hits = searchSettings("notification area", INDEX);
    expect(hits[0].entry.id).toBe("tray");
  });

  it("requires every word of the query", () => {
    expect(searchSettings("speed startup", INDEX)).toEqual([]);
  });

  it("returns nothing for an empty query", () => {
    expect(searchSettings("   ", INDEX)).toEqual([]);
  });

  it("returns nothing when nothing matches", () => {
    expect(searchSettings("zzzz", INDEX)).toEqual([]);
  });

  it("ranks a name match above a description match", () => {
    const hits = searchSettings("startup", INDEX);
    expect(hits[0].entry.id).toBe("startup");
  });

  it("caps a broad result set after scoring", () => {
    const many = Array.from({ length: 80 }, (_, index) => ({
      id: `entry-${index}`,
      page: "general",
      section: "Many",
      title: `Download option ${index}`,
      description: "A download setting.",
      keywords: ["download"],
    }));
    const hits = searchSettings("download", buildSettingsSearchIndex(many, LABELS));
    expect(hits).toHaveLength(40);
    expect(hits[0].score).toBeGreaterThan(0);
  });
});

describe("groupSettingsHits", () => {
  it("groups by page in the sidebar order and numbers rows on screen", () => {
    const hits = searchSettings("a", buildSettingsSearchIndex(ENTRIES, LABELS));
    const groups = groupSettingsHits(
      hits,
      LABELS,
      ["general", "connections"],
    );
    expect(groups.map((group) => group.page)).toEqual(["general", "connections"]);

    const indices = groups.flatMap((group) =>
      group.hits.map((hit) => hit.index),
    );
    expect(indices).toEqual(indices.map((_, index) => index));
  });
});

describe("search performance", () => {
  it("stays linear over a large catalogue", () => {
    const entries: SettingsSearchEntry[] = Array.from(
      { length: 5000 },
      (_, index) => ({
        id: `bulk-${index}`,
        page: index % 2 === 0 ? "general" : "connections",
        section: "Bulk",
        title: `Download setting number ${index}`,
        description: "A bulk setting for a large collection.",
        keywords: ["download", "bulk", `item ${index}`],
      }),
    );
    const index = buildSettingsSearchIndex(entries, LABELS);

    const started = performance.now();
    for (let run = 0; run < 50; run += 1) {
      searchSettings("download bulk", index);
    }
    const elapsed = performance.now() - started;

    expect(elapsed).toBeLessThan(1000);
  });
});
