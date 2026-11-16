import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { TorrentPieces } from "@/components/windows/torrent/torrent-pieces";
import type { TorrentJob } from "@/lib/ipc-torrent";

// jsdom has no canvas, and the grid is painted on one. The grid copes with
// that, and the test does not need the painting.
Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
  value: () => null,
});

function job(patch: Partial<TorrentJob> = {}): TorrentJob {
  return {
    infoHash: "hash",
    name: "Example",
    directory: "C:\\Downloads",
    magnet: null,
    files: [],
    selected: [],
    pieceMap: { pieceLength: 262_144, count: 10, have: 3, totalLength: 2_621_440 },
    pieceBits: "oA==",
    pieceInflight: 4,
    peers: [],
    trackers: [],
    status: { state: "downloading" },
    downloaded: 1024 * 1024,
    uploaded: 0,
    totalLength: 2_621_440,
    seeding: false,
    downSpeed: 0,
    upSpeed: 0,
    eta: null,
    seeders: 0,
    leechers: 0,
    addedAt: 0,
    error: null,
    ...patch,
  };
}

describe("TorrentPieces", () => {
  it("says how many pieces are on disk", () => {
    render(<TorrentPieces job={job()} />);

    expect(screen.getByText("3 of 10 pieces, 1.00 MB on disk")).toBeTruthy();
  });

  it("gives the grid one name that holds the counts", () => {
    render(<TorrentPieces job={job()} />);

    const grid = screen.getByRole("img");
    const name = grid.getAttribute("aria-label") ?? "";
    expect(name).toContain("3 of 10 pieces on disk");
    expect(name).toContain("1.00 MB");
    expect(name).toContain("Piece 5 is coming in");
  });

  it("keeps the grid out of the tab order", () => {
    render(<TorrentPieces job={job()} />);

    expect(screen.getAllByRole("img").length).toBe(1);
    expect(document.querySelectorAll("canvas[tabindex]").length).toBe(0);
  });

  it("says the pieces are not known yet when a magnet has not said", () => {
    render(
      <TorrentPieces
        job={job({
          pieceMap: { pieceLength: 0, count: 0, have: 0, totalLength: 0 },
          pieceBits: "",
          pieceInflight: null,
        })}
      />,
    );

    expect(screen.getByText(/not known yet/)).toBeTruthy();
    expect(screen.queryByRole("img")).toBeNull();
  });
});
