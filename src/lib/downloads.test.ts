import { describe, expect, it } from "vitest";

import { categoryTargetFolder } from "@/lib/downloads";
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
