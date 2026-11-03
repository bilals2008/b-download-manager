import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { FileTypeIcon } from "@/components/icons/desktop-icons";
import { defaultCategoryIcon } from "@/lib/category-icons";

describe("FileTypeIcon", () => {
  it("draws the file itself even when the category has its own icon", () => {
    const { container } = render(
      <FileTypeIcon
        category="Video"
        icon={defaultCategoryIcon("Video")}
        name="clip.mp4"
      />,
    );

    expect(container.querySelector("img")).not.toBeNull();
    expect(container.querySelector("svg")).toBeNull();
  });

  it("draws the category glyph when no file is named", () => {
    const { container } = render(
      <FileTypeIcon category="Video" icon={defaultCategoryIcon("Video")} />,
    );

    expect(container.querySelector("svg")).not.toBeNull();
    expect(container.querySelector("img")).toBeNull();
  });
});
