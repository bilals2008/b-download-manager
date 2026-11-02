import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { DownloadProgress } from "@/components/workspace/download-progress";
import { PROGRESS_FILL } from "@/components/ui/progress-fill";

describe("DownloadProgress", () => {
  it("exposes the label and the rounded percentage to assistive tech", () => {
    render(<DownloadProgress value={42.6} status="downloading" label="Ubuntu ISO" />);

    const bar = screen.getByRole("progressbar", { name: "Ubuntu ISO" });
    expect(bar.getAttribute("aria-valuenow")).toBe("43");
    expect(bar.getAttribute("aria-valuetext")).toBe("42.60%");
    expect(screen.getByText("42.60%")).toBeTruthy();
  });

  it("clamps values outside 0-100", () => {
    const { rerender } = render(
      <DownloadProgress value={-20} status="paused" label="Paused file" />,
    );
    expect(
      screen.getByRole("progressbar").getAttribute("aria-valuenow"),
    ).toBe("0");

    rerender(<DownloadProgress value={180} status="completed" label="Done file" />);
    expect(
      screen.getByRole("progressbar").getAttribute("aria-valuenow"),
    ).toBe("100");
  });

  it("maps every download status to its fill", () => {
    const cases = [
      ["downloading", PROGRESS_FILL.active],
      ["completed", PROGRESS_FILL.complete],
      ["paused", PROGRESS_FILL.paused],
      ["queued", PROGRESS_FILL.queued],
      ["stopped", PROGRESS_FILL.stopped],
      ["error", PROGRESS_FILL.failed],
    ] as const;

    for (const [status, fill] of cases) {
      const { container, unmount } = render(
        <DownloadProgress value={50} status={status} label={status} />,
      );
      const track = container.querySelector('[role="progressbar"]');
      const inner = track?.firstElementChild;
      expect(inner?.className).toContain(fill.split(" ")[0]);
      unmount();
    }
  });
});
