import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { SettingsSearch } from "@/components/settings/settings-search";

function setup(page = "general") {
  const onNavigate = vi.fn();
  const user = userEvent.setup();
  const view = render(
    <SettingsSearch page={page} onNavigate={onNavigate}>
      <p>Current settings page</p>
    </SettingsSearch>,
  );
  return { onNavigate, user, view };
}

describe("SettingsSearch", () => {
  it("shows the page until a search is typed", async () => {
    const { user } = setup();
    expect(screen.getByText("Current settings page")).toBeTruthy();

    await user.type(screen.getByRole("combobox"), "theme");

    expect(await screen.findByText("Auto follows your system.")).toBeTruthy();
    expect(screen.queryByText("Current settings page")).toBeNull();
    expect(screen.getByText("Appearance")).toBeTruthy();
  });

  it("clears with the button and brings the page back", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("combobox"), "theme");

    await user.click(await screen.findByRole("button", { name: "Clear search" }));

    expect(screen.getByRole("combobox")).toHaveProperty("value", "");
    expect(screen.getByText("Current settings page")).toBeTruthy();
  });

  it("clears with Escape without closing the window", async () => {
    const { user } = setup();
    const input = screen.getByRole("combobox");
    await user.type(input, "theme");

    await user.keyboard("{Escape}");

    expect(input).toHaveProperty("value", "");
    expect(screen.getByText("Current settings page")).toBeTruthy();
  });

  it("gives way when the sidebar opens another page", async () => {
    const { user, onNavigate, view } = setup("general");
    await user.type(screen.getByRole("combobox"), "theme");
    await screen.findByText("Auto follows your system.");

    view.rerender(
      <SettingsSearch page="appearance" onNavigate={onNavigate}>
        <p>Current settings page</p>
      </SettingsSearch>,
    );

    expect(screen.getByRole("combobox")).toHaveProperty("value", "");
    expect(screen.getByText("Current settings page")).toBeTruthy();
  });

  it("shows a no-results message", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("combobox"), "zzzzz");

    expect(await screen.findByText(/No setting matches/)).toBeTruthy();
    expect(screen.getByText(/speed, folder or shortcut/)).toBeTruthy();
  });

  it("opens the chosen result with the mouse", async () => {
    const { user, onNavigate } = setup();
    await user.type(screen.getByRole("combobox"), "theme");

    await user.click(await screen.findByRole("option", { name: /Theme/ }));

    expect(onNavigate).toHaveBeenCalledTimes(1);
    expect(onNavigate.mock.calls[0][0]).toMatchObject({
      id: "setting-theme",
      page: "appearance",
    });
  });

  it("moves with the arrows and opens with Enter", async () => {
    const { user, onNavigate } = setup();
    const input = screen.getByRole("combobox");
    await user.type(input, "speed limit");
    await screen.findByRole("option", { name: /^Limit the speed/ });

    await user.keyboard("{ArrowDown}");
    const options = screen.getAllByRole("option");
    expect(options[1].getAttribute("aria-selected")).toBe("true");

    await user.keyboard("{Enter}");

    expect(onNavigate).toHaveBeenCalledTimes(1);
    expect(onNavigate.mock.calls[0][0]).toMatchObject({
      id: "setting-speed-limit-toggle",
    });
  });

  it("names the active row for assistive technology", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("combobox"), "theme");

    const option = await screen.findByRole("option", { name: /Theme/ });
    expect(option.getAttribute("aria-selected")).toBe("true");
    expect(
      screen.getByRole("combobox").getAttribute("aria-activedescendant"),
    ).toBe(option.id);
  });
});
