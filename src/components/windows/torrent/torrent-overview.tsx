import type { ReactNode } from "react";

import { formatDate, formatEta, formatSize } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";

import { PieceGrid } from "./piece-grid";
import { Legend } from "./torrent-pieces";
import { statusSentence } from "./torrent-status";

interface TorrentOverviewProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/** One fact of a job, with its name and its value. */
function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-0.5">
      <dt className="shrink-0 text-chrome-micro text-chrome-muted">{label}</dt>
      <dd className="min-w-0 truncate text-right text-xs tabular-nums text-chrome-menu-text">
        {value}
      </dd>
    </div>
  );
}

/** A named set of facts, so each number has a home in the pane. */
function FactsGroup({
  caption,
  children,
}: {
  caption: string;
  children: ReactNode;
}) {
  return (
    <section>
      <p className="mb-1 text-chrome-caption tracking-wide text-chrome-muted uppercase">
        {caption}
      </p>
      <dl>{children}</dl>
    </section>
  );
}

/**
 * The summary page of a job: its pieces drawn at a glance, and its numbers
 * grouped into two columns so the whole pane carries information.
 */
export function TorrentOverview({ job }: TorrentOverviewProps) {
  const count = job.pieceMap.count;
  const have = job.pieceMap.have;
  const onDisk = formatSize(job.downloaded);
  const connected = job.peers.filter((peer) => peer.connected).length;

  const files =
    job.files.length === 0
      ? "Not known yet"
      : job.selected.length > 0 && job.selected.length < job.files.length
        ? `${job.selected.length} of ${job.files.length} selected`
        : `${job.files.length}`;
  const pieceSize =
    job.pieceMap.pieceLength > 0 ? formatSize(job.pieceMap.pieceLength) : "—";
  const status = job.error ? job.error : statusSentence(job.status);

  const coming =
    job.pieceInflight === null
      ? ""
      : ` Piece ${job.pieceInflight + 1} is coming in.`;
  const gridLabel = `${have} of ${count} pieces on disk, ${onDisk}.${coming}`;

  return (
    <div className="flex flex-col gap-4 pb-3">
      <section>
        <div className="flex items-baseline justify-between gap-3">
          <p className="text-chrome-caption tracking-wide text-chrome-muted uppercase">
            Pieces on disk
          </p>
          <p className="text-xs tabular-nums text-chrome-menu-text">
            {count > 0 ? `${have} of ${count} · ${onDisk}` : "Not known yet"}
          </p>
        </div>
        {count > 0 ? (
          <>
            <div className="mt-2">
              <PieceGrid
                bits={job.pieceBits}
                count={count}
                inflight={job.pieceInflight}
                label={gridLabel}
              />
            </div>
            <Legend />
          </>
        ) : (
          <p className="mt-1 text-chrome-micro text-chrome-muted">
            A magnet learns the pieces from its peers.
          </p>
        )}
      </section>

      <div className="grid grid-cols-2 gap-x-6 gap-y-4">
        <div className="flex flex-col gap-4">
          <FactsGroup caption="Transfer">
            <Fact label="Total size" value={formatSize(job.totalLength)} />
            <Fact label="Downloaded" value={formatSize(job.downloaded)} />
            <Fact label="Uploaded" value={formatSize(job.uploaded)} />
            <Fact
              label="Time left"
              value={job.eta === null ? "—" : formatEta(job.eta)}
            />
          </FactsGroup>
          <FactsGroup caption="Storage">
            <Fact label="Files" value={files} />
            <Fact label="Saved in" value={job.directory} />
            <Fact label="Added" value={formatDate(job.addedAt)} />
            <Fact label="Source" value={job.magnet ? "Magnet link" : "Torrent file"} />
          </FactsGroup>
        </div>
        <div className="flex flex-col gap-4">
          <FactsGroup caption="Peers & trackers">
            <Fact label="Connected" value={`${connected}`} />
            <Fact label="Known" value={`${job.peers.length}`} />
            <Fact label="Seeders" value={`${job.seeders}`} />
            <Fact label="Leechers" value={`${job.leechers}`} />
            <Fact label="Trackers" value={`${job.trackers.length}`} />
          </FactsGroup>
          <FactsGroup caption="Job">
            <Fact label="Status" value={status} />
            <Fact label="Pieces on disk" value={`${have} of ${count}`} />
            <Fact label="Piece size" value={pieceSize} />
          </FactsGroup>
        </div>
      </div>
    </div>
  );
}
