import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { UpdateProgressBody } from "@/components/updates/update-meter";
import type { UpdateRun } from "@/state/update-store";

const FOUND = {
  available: true,
  currentVersion: "0.3.1",
  version: "0.4.0",
  date: null,
  notes: null,
  error: null,
};

function run(patch: Partial<UpdateRun>): UpdateRun {
  return {
    phase: "downloading",
    update: FOUND,
    downloaded: 0,
    total: null,
    speed: null,
    error: null,
    ...patch,
  };
}

describe("UpdateProgressBody", () => {
  it("shows the real byte share when the server named a size", () => {
    render(
      <UpdateProgressBody
        run={run({ downloaded: 12 * 1024 * 1024, total: 32 * 1024 * 1024 })}
      />,
    );

    const bar = screen.getByRole("progressbar", { name: "Update progress" });
    expect(bar.getAttribute("aria-valuenow")).toBe("38");
    expect(screen.getByText("Downloading the update")).toBeTruthy();
    expect(screen.getByText("12 MB of 32 MB")).toBeTruthy();
    expect(screen.getByText("38%")).toBeTruthy();
  });

  it("shows the measured speed beside the bytes", () => {
    render(
      <UpdateProgressBody
        run={run({
          downloaded: 12 * 1024 * 1024,
          total: 32 * 1024 * 1024,
          speed: 5 * 1024 * 1024,
        })}
      />,
    );

    expect(screen.getByText("12 MB of 32 MB · 5.00 MB/s")).toBeTruthy();
  });

  it("runs an indeterminate segment when the total is unknown", () => {
    render(<UpdateProgressBody run={run({ downloaded: 900_000 })} />);

    const bar = screen.getByRole("progressbar", { name: "Update progress" });
    expect(bar.getAttribute("aria-valuenow")).toBeNull();
    expect(bar.getAttribute("aria-valuetext")).toMatch(/size unknown/);
    expect(screen.getByText("879 KB downloaded")).toBeTruthy();
    expect(screen.queryByText(/%$/)).toBeNull();
  });

  it("says the signature is being checked, not that it passed", () => {
    render(
      <UpdateProgressBody
        run={run({ phase: "verifying", downloaded: 32 * 1024 * 1024, total: 32 * 1024 * 1024 })}
      />,
    );

    expect(screen.getByText("Checking the download is genuine")).toBeTruthy();
    expect(
      screen.getByText(/signature is being checked/),
    ).toBeTruthy();
    expect(
      screen.getByRole("progressbar").getAttribute("aria-valuenow"),
    ).toBe("100");
  });

  it("keeps the failure sentence the backend gave", () => {
    render(
      <UpdateProgressBody
        run={run({ phase: "error", error: "the signature did not match" })}
      />,
    );

    expect(screen.getByText("the signature did not match")).toBeTruthy();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("offers the restart only after the installer has been started", () => {
    render(
      <UpdateProgressBody
        run={run({ phase: "restart-ready", downloaded: 32 * 1024 * 1024, total: 32 * 1024 * 1024 })}
      />,
    );

    expect(
      screen.getByText("The new version is installed. Restart to finish."),
    ).toBeTruthy();
  });
});
