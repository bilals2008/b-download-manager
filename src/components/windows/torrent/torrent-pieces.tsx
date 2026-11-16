import { formatSize } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";

import { PieceGrid } from "./piece-grid";

interface TorrentPiecesProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/** What the three kinds of cell mean, in words and not only in colour. */
export function Legend() {
  return (
    <ul className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-chrome-micro text-chrome-muted">
      <li className="flex items-center gap-1.5">
        <span aria-hidden="true" className="size-1.5 bg-chrome-focus" />
        On disk
      </li>
      <li className="flex items-center gap-1.5">
        <span aria-hidden="true" className="size-1.5 bg-chrome-line" />
        Missing
      </li>
      <li className="flex items-center gap-1.5">
        <span aria-hidden="true" className="size-1.5 bg-chrome-focus/55" />
        Coming in
      </li>
    </ul>
  );
}

/**
 * The pieces of a job: a cell for each one, filled when the piece is on
 * disk, dimmed while it is on its way.
 *
 * A magnet does not know how many pieces it has until a peer has told it, so
 * the page says so rather than drawing an empty grid.
 */
export function TorrentPieces({ job }: TorrentPiecesProps) {
  const count = job.pieceMap.count;
  const have = job.pieceMap.have;

  if (count === 0) {
    return (
      <p className="py-6 text-center text-xs text-chrome-muted">
        The pieces of this torrent are not known yet. A magnet asks its peers
        for them.
      </p>
    );
  }

  const onDisk = formatSize(job.downloaded);
  const coming =
    job.pieceInflight === null ? "" : ` Piece ${job.pieceInflight + 1} is coming in.`;
  const label = `${have} of ${count} pieces on disk, ${onDisk}.${coming}`;

  return (
    <div className="pb-2">
      <p className="text-xs text-chrome-menu-text">
        {`${have} of ${count} pieces, ${onDisk} on disk`}
      </p>
      <Legend />
      <div className="mt-2">
        <PieceGrid
          bits={job.pieceBits}
          count={count}
          inflight={job.pieceInflight}
          label={label}
        />
      </div>
    </div>
  );
}
