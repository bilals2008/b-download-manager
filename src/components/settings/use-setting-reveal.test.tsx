import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
    const user = userEvent.setup();
    const scrollIntoView = vi.fn();
    const row = document.createElement("div");
    row.id = "searched-row";
    row.tabIndex = -1;
    row.setAttribute("data-setting-row", "");
    row.scrollIntoView = scrollIntoView;
    document.body.appendChild(row);

    render(<Probe />);
    await user.click(screen.getByRole("button", { name: "Jump" }));

    expect(scrollIntoView).toHaveBeenCalledWith({ block: "center" });
    expect(row.classList.contains("setting-flash")).toBe(true);
    expect(document.activeElement).toBe(row);

    await new Promise((resolve) => setTimeout(resolve, TEST_FLASH_MS + 20));

    expect(row.classList.contains("setting-flash")).toBe(false);
  });

  it("leaves a target that has not drawn alone", async () => {
    const user = userEvent.setup();
    render(<Probe />);

    await user.click(screen.getByRole("button", { name: "Jump" }));

    expect(document.querySelector(".setting-flash")).toBeNull();
  });
});
