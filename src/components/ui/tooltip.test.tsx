import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Tooltip } from "@/components/ui/tooltip";

describe("Tooltip", () => {
  it("opens on keyboard focus and describes the control", async () => {
    render(
      <Tooltip content="Delete the file" shortcut="Del">
        <button type="button">Delete</button>
      </Tooltip>,
    );

    const button = screen.getByRole("button", { name: "Delete" });
    expect(button.parentElement?.getAttribute("aria-describedby")).toBeNull();

    button.focus();
    const tip = await screen.findByRole("tooltip");
    expect(screen.getByText("Delete the file")).toBeTruthy();
    expect(screen.getByText("Del")).toBeTruthy();
    expect(button.parentElement?.getAttribute("aria-describedby")).toBe(tip.id);
  });

  it("closes on Escape without moving the focus", async () => {
    render(
      <Tooltip content="Delete the file">
        <button type="button">Delete</button>
      </Tooltip>,
    );

    const button = screen.getByRole("button", { name: "Delete" });
    button.focus();
    await screen.findByRole("tooltip");

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("tooltip")).toBeNull());
    expect(document.activeElement).toBe(button);
  });

  it("says nothing when it has nothing to say", () => {
    render(
      <Tooltip content="">
        <button type="button">Delete</button>
      </Tooltip>,
    );

    const button = screen.getByRole("button", { name: "Delete" });
    button.focus();
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
