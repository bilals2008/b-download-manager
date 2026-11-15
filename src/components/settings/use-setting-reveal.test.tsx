import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  useSettingReveal,
  type SettingTarget,
} from "@/components/settings/use-setting-reveal";

/** A short flash keeps the test quick; the window uses the real duration. */
const TEST_FLASH_MS = 20;

function Probe() {
  const [target, setTarget] = useState<SettingTarget | null>(null);
  useSettingReveal(target, TEST_FLASH_MS);
  return (
    <button type="button" onClick={() => setTarget({ anchor: "searched-row" })}>
      Jump
    </button>
  );
}

afterEach(() => {
  document.getElementById("searched-row")?.remove();
});

describe("useSettingReveal", () => {
  it("scrolls to the row, focuses it and flashes it briefly", async () => {
    const scrollIntoView = vi.fn();
    const row = document.createElement("div");
    row.id = "searched-row";
    row.tabIndex = -1;
    row.setAttribute("data-setting-row", "");
    row.scrollIntoView = scrollIntoView;
    document.body.appendChild(row);

    render(<Probe />);
    // A synchronous click flushes the effect in the same tick, so the ring is
    // asserted before any timer can run. An async userEvent click has a tail
    // that, under load, can outlast the short flash and remove the ring early.
    fireEvent.click(screen.getByRole("button", { name: "Jump" }));

    expect(scrollIntoView).toHaveBeenCalledWith({ block: "center" });
    expect(row.classList.contains("setting-flash")).toBe(true);
    expect(document.activeElement).toBe(row);

    // Both are real timers: the removal (20ms) always fires before this wait
    // (40ms) regardless of how busy the loop is, so the assertion is stable.
    await new Promise((resolve) => setTimeout(resolve, TEST_FLASH_MS + 20));

    expect(row.classList.contains("setting-flash")).toBe(false);
  });

  it("leaves a target that has not drawn alone", () => {
    render(<Probe />);

    fireEvent.click(screen.getByRole("button", { name: "Jump" }));

    expect(document.querySelector(".setting-flash")).toBeNull();
  });
});