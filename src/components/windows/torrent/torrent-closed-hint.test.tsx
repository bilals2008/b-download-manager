import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const listeners = new Map<string, () => void>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (event: string, handler: () => void) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  },
}));

import {
  TORRENT_CLOSED,
  TorrentClosedHint,
} from "@/components/windows/torrent/torrent-closed-hint";

describe("TorrentClosedHint", () => {
  beforeEach(() => {
    listeners.clear();
    // The hint follows the backend, which only exists in the desktop app.
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("says where the window went, then goes away by itself", async () => {
    render(<TorrentClosedHint />);
    await act(async () => {});

    expect(screen.queryByText("Torrents window closed")).toBeNull();

    await act(async () => listeners.get(TORRENT_CLOSED)?.());

    expect(screen.getByText("Torrents window closed")).toBeTruthy();
    expect(
      screen.getByText(/Tasks › Torrents/, { exact: false }),
    ).toBeTruthy();

    act(() => vi.advanceTimersByTime(12_000));

    expect(screen.queryByText("Torrents window closed")).toBeNull();
  });

  it("waits while the main window is in the tray", async () => {
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    render(<TorrentClosedHint />);
    await act(async () => {});
    await act(async () => listeners.get(TORRENT_CLOSED)?.());

    act(() => vi.advanceTimersByTime(60_000));

    expect(screen.getByText("Torrents window closed")).toBeTruthy();
    hidden.mockRestore();
  });
});
