import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useShellShortcuts } from "@/components/shell/use-shell-shortcuts";
import { invalidateShortcutCache, setShortcutBinding } from "@/lib/shortcuts";
import type { ShortcutId } from "@/lib/shortcuts";

afterEach(() => {
  cleanup();
  localStorage.clear();
  invalidateShortcutCache();
});

function Probe({ handlers }: { handlers: Partial<Record<ShortcutId, () => void>> }) {
  useShellShortcuts(handlers);
  return (
    <div>
      <input data-testid="field" />
      <div
        data-testid="viewport"
        data-shortcut-scope="list"
        tabIndex={0}
        role="grid"
        aria-label="Download list"
      />
      <dialog data-testid="modal">
        <input data-testid="modal-field" />
      </dialog>
    </div>
  );
}

function ctrl(
  element: Element | Window,
  key: string,
  modifiers: { shift?: boolean; alt?: boolean } = {},
) {
  fireEvent.keyDown(element, {
    key,
    ctrlKey: true,
    shiftKey: modifiers.shift ?? false,
    altKey: modifiers.alt ?? false,
  });
}

describe("the shell shortcut dispatcher", () => {
  it("answers the shipped keys from anywhere in the window", () => {
    const add = vi.fn();
    const find = vi.fn();
    render(<Probe handlers={{ "add-download": add, find }} />);

    ctrl(window, "n");
    expect(add).toHaveBeenCalledTimes(1);
    ctrl(window, "f");
    expect(find).toHaveBeenCalledTimes(1);
  });

  it("does not touch normal text input", () => {
    const add = vi.fn();
    const selectAll = vi.fn();
    const screen = render(
      <Probe handlers={{ "add-download": add, "select-all": selectAll }} />,
    );
    const field = screen.getByTestId("field");

    ctrl(field, "n");
    ctrl(field, "a");
    expect(add).not.toHaveBeenCalled();
    expect(selectAll).not.toHaveBeenCalled();
  });

  it("keeps function keys working from the Find box", () => {
    const findNext = vi.fn();
    const screen = render(<Probe handlers={{ "find-next": findNext }} />);
    const field = screen.getByTestId("field");

    fireEvent.keyDown(field, { key: "F3" });
    expect(findNext).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(field, { key: "F3", shiftKey: true });
    // Shift+F3 belongs to find-previous, which has no handler here.
    expect(findNext).toHaveBeenCalledTimes(1);
  });

  it("stands down while a window of its own has the keyboard", () => {
    const add = vi.fn();
    const screen = render(<Probe handlers={{ "add-download": add }} />);
    const modal = screen.getByTestId("modal");

    ctrl(modal, "n");
    ctrl(modal.querySelector("input")!, "n");
    fireEvent.keyDown(modal, { key: "F3" });
    expect(add).not.toHaveBeenCalled();
  });

  it("keeps the Delete key local to the list", () => {
    const remove = vi.fn();
    const screen = render(<Probe handlers={{ "remove-selected": remove }} />);

    fireEvent.keyDown(screen.getByTestId("viewport"), { key: "Delete" });
    expect(remove).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(screen.getByTestId("field"), { key: "Delete" });
    fireEvent.keyDown(window, { key: "Delete" });
    expect(remove).toHaveBeenCalledTimes(1);
  });

  it("follows a rebind and stops answering the old key", () => {
    setShortcutBinding("add-download", "Ctrl+Alt+K");
    const add = vi.fn();
    render(<Probe handlers={{ "add-download": add }} />);

    fireEvent.keyDown(window, { key: "k", ctrlKey: true, altKey: true });
    expect(add).toHaveBeenCalledTimes(1);
    ctrl(window, "n");
    expect(add).toHaveBeenCalledTimes(1);
  });

  it("says nothing about a row the reader cleared", () => {
    setShortcutBinding("add-download", null);
    const add = vi.fn();
    render(<Probe handlers={{ "add-download": add }} />);

    ctrl(window, "n");
    expect(add).not.toHaveBeenCalled();
  });

  it("leaves a key alone when the app has no handler for it", () => {
    render(<Probe handlers={{}} />);
    const event = new KeyboardEvent("keydown", {
      key: "n",
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it("lets a control that already answered the key keep it", () => {
    const add = vi.fn();
    const screen = render(<Probe handlers={{ "add-download": add }} />);
    const viewport = screen.getByTestId("viewport");
    viewport.addEventListener("keydown", (event) => event.preventDefault());

    ctrl(viewport, "n");
    expect(add).not.toHaveBeenCalled();
  });
});
