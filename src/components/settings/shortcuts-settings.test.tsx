import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ShortcutsSettings } from "@/components/settings/shortcuts-settings";
import {
  getEffectiveBinding,
  invalidateShortcutCache,
  setShortcutBinding,
} from "@/lib/shortcuts";

beforeEach(() => {
  localStorage.clear();
  invalidateShortcutCache();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  invalidateShortcutCache();
});

function recordButton(label: string): HTMLButtonElement {
  return screen.getByRole("button", {
    name: `Change the shortcut for ${label}`,
  });
}

describe("the shortcuts page", () => {
  it("shows each command with its current key", () => {
    render(<ShortcutsSettings />);
    expect(recordButton("Add a download").textContent).toBe("Ctrl+N");
    expect(recordButton("Search downloads").textContent).toBe("Ctrl+F");
    expect(recordButton("Remove selected").textContent).toBe("Del");
    // The row that ships without a key says so instead of pretending.
    expect(recordButton("Stop the queue").textContent).toBe("Not set");
    // The fixed keys are on the page too, as plain caps.
    expect(screen.getByText("Shift+F10")).toBeTruthy();
    expect(screen.getByText("Alt+T")).toBeTruthy();
  });

  it("records a new key when the row is clicked and a combo is pressed", () => {
    render(<ShortcutsSettings />);
    const button = recordButton("Add a download");

    fireEvent.click(button);
    expect(button.textContent).toBe("Press keys…");

    fireEvent.keyDown(button, { key: "j", ctrlKey: true });
    expect(button.textContent).toBe("Ctrl+J");
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+J");
    invalidateShortcutCache();
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+J");
  });

  it("refuses a key another command holds and says who has it", () => {
    render(<ShortcutsSettings />);
    const button = recordButton("Add a download");

    fireEvent.click(button);
    fireEvent.keyDown(button, { key: "f", ctrlKey: true });

    expect(
      screen.getByRole("alert").textContent,
    ).toContain("Already used by Search downloads");
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+N");
    expect(getEffectiveBinding("find")).toBe("Ctrl+F");
    // Still recording, so the reader can try a different key.
    expect(button.textContent).toBe("Press keys…");
  });

  it("refuses the menu bar's Alt letters and the system keys", () => {
    render(<ShortcutsSettings />);
    const button = recordButton("Open Settings");

    fireEvent.click(button);
    fireEvent.keyDown(button, { key: "d", altKey: true });
    expect(screen.getByRole("alert").textContent).toContain("opens a menu");

    fireEvent.keyDown(button, { key: "F4", altKey: true });
    expect(screen.getByRole("alert").textContent).toContain("system");
    expect(getEffectiveBinding("open-settings")).toBe("Ctrl+,");
  });

  it("clears a row with Backspace and puts it back with Reset", () => {
    render(<ShortcutsSettings />);
    const button = recordButton("Find next");

    fireEvent.click(button);
    fireEvent.keyDown(button, { key: "Backspace" });
    expect(button.textContent).toBe("Not set");
    expect(getEffectiveBinding("find-next")).toBeNull();

    // The reset control only appears once the row has moved.
    const reset = screen.getByRole("button", {
      name: "Reset the shortcut for Find next",
    });
    fireEvent.click(reset);
    expect(recordButton("Find next").textContent).toBe("F3");
    expect(
      screen.queryByRole("button", {
        name: "Reset the shortcut for Find next",
      }),
    ).toBeNull();
  });

  it("cancels the recording with Esc without touching the key", () => {
    render(<ShortcutsSettings />);
    const button = recordButton("Deselect all");

    fireEvent.click(button);
    fireEvent.keyDown(button, { key: "Escape" });
    expect(button.textContent).toBe("Ctrl+Shift+A");
    expect(getEffectiveBinding("clear-selection")).toBe("Ctrl+Shift+A");
  });

  it("restores every row at once", () => {
    setShortcutBinding("add-download", "Ctrl+Shift+K");
    setShortcutBinding("find", "Ctrl+Shift+L");
    render(<ShortcutsSettings />);

    fireEvent.click(screen.getByRole("button", { name: "Restore defaults" }));
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+N");
    expect(getEffectiveBinding("find")).toBe("Ctrl+F");
    expect(screen.queryByRole("button", { name: "Restore defaults" })).toBeNull();
  });

  it("keeps the whole page's keys distinct after editing", () => {
    render(<ShortcutsSettings />);
    const button = recordButton("Move to the next section");

    fireEvent.click(button);
    // Ctrl+Tab is its own; take the other row's forward key away from it.
    fireEvent.keyDown(button, { key: "Tab", ctrlKey: true, shiftKey: true });
    expect(screen.getByRole("alert").textContent).toContain(
      "Already used by Move to the previous section",
    );
  });
});
