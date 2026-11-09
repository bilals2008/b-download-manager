import { describe, expect, it } from "vitest";

import {
  categoryTargetFolder,
  progressOf,
  type Download,
} from "@/lib/downloads";
import { DEFAULT_CATEGORIES } from "@/lib/categories";

/** The built-ins with one folder set, as a properties window would leave them. */
const WITH_FOLDERS = DEFAULT_CATEGORIES.map((category) =>
  category.name === "Video" ? { ...category, folder: "D:\\Media\\Films" } : category,
);

describe("categoryTargetFolder", () => {
  it("lets the folder set on the category win over the derived one", () => {
    expect(categoryTargetFolder(WITH_FOLDERS, "Video", "C:\\Downloads")).toBe(
      "D:\\Media\\Films",
    );
  });

  it("sends a category without a folder under the default save folder", () => {
    expect(categoryTargetFolder(DEFAULT_CATEGORIES, "Music", "C:\\Downloads")).toBe(
      "C:\\Downloads\\Music",
    );
  });

  it("keeps General and Other on the default save folder itself", () => {
    expect(categoryTargetFolder(DEFAULT_CATEGORIES, "General", "C:\\Downloads")).toBe("");
    expect(categoryTargetFolder(DEFAULT_CATEGORIES, "Other", "C:\\Downloads")).toBe("");
  });

  it("holds the derived folder back while no default is known", () => {
    expect(categoryTargetFolder(DEFAULT_CATEGORIES, "Music", "")).toBe("");
  });
});

/** A row with every field, so one test can change only what it is about. */
function row(overrides: Partial<Download> = {}): Download {
  return {
    id: 1,
    fileName: "Film.mp4",
    url: "https://example.org/film.mp4",
    category: "Video",
    size: 100,
    downloaded: 0,
    status: "downloading",
    speed: 0,
    eta: 0,
    addedAt: 0,
    lastTryAt: 0,
    savePath: "",
    dest: "",
    referer: null,
    connections: 1,
    error: null,
    resumable: null,
    description: null,
    ...overrides,
  };
}

describe("progressOf", () => {
  it("follows the coordinator fraction of a media row, not its moving bytes", () => {
    // The picture alone has reached its own total while the sound has not
    // started, so the bytes would read 100%. The fraction is the whole job.
    const media = row({
      id: -999_999_999,
      mediaJob: 1,
      mediaProgress: 0.42,
      size: 10,
      downloaded: 10,
    });
    expect(progressOf(media)).toBeCloseTo(42);
  });

  it("keeps a finished row at 100 whatever the row holds", () => {
    expect(progressOf(row({ status: "completed", size: 0, downloaded: 0 }))).toBe(
      100,
    );
  });
});
