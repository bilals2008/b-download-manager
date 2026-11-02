import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ReleaseNotes } from "@/components/ui/release-notes";

describe("ReleaseNotes", () => {
  it("draws bullets and headings as text, not as markdown marks", () => {
    const notes = [
      "## What changed",
      "",
      "- Faster **resuming** after a restart",
      "- A download can be renamed from the list",
    ].join("\n");

    const { container } = render(<ReleaseNotes notes={notes} />);

    expect(screen.getByText("What changed")).toBeTruthy();
    expect(screen.getByText("Faster resuming after a restart")).toBeTruthy();
    expect(container.textContent).not.toContain("##");
    expect(container.textContent).not.toContain("**");
    expect(container.querySelector('[aria-hidden="true"]')?.textContent).toBe("•");
  });

  it("marks a heading so it stands apart from the lines", () => {
    const { container } = render(<ReleaseNotes notes={"# A heading\nplain line"} />);
    const [heading, line] = Array.from(container.querySelectorAll("p"));

    expect(heading.className).toContain("font-medium");
    expect(line.className ?? "").not.toContain("font-medium");
  });

  it("renders nothing when there are no notes", () => {
    const { container } = render(<ReleaseNotes notes={"  \n\n "} />);
    expect(container.querySelectorAll("p")).toHaveLength(0);
  });
});
