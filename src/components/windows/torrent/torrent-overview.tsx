import { Tooltip } from "@/components/ui/tooltip";
import { formatEta, formatSize, formatSpeed } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";

import { PieceGrid } from "./piece-grid";
import { Legend } from "./torrent-pieces";

interface TorrentOverviewProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/**
 * One line of the summary: its name on the left, its value on the right.
 *
 * The value gets the width that is left and is cut with an ellipsis, so a
 * long folder path ends inside the column instead of running out of the
 * window.
 */
function Row({
  label,
  value,
  title,
}: {
  label: string;
  value: string;
  title?: string;
}) {
  const text = (
    <span className="block truncate text-right text-xs tabular-nums text-chrome-menu-text">
      {value}
    </span>
  );
  return (
    <div className="flex min-w-0 items-baseline justify-between gap-3 py-1">
      <dt className="shrink-0 text-chrome-micro text-chrome-muted">{label}</dt>
      {title ? (
        <Tooltip content={title} side="left" className="min-w-0 flex-1 justify-end">
          {text}
        </Tooltip>
      ) : (
        <span className="min-w-0 flex-1">{text}</span>
      )}
    </div>
  );
}

/**
 * What the swarm is doing, in the words a reader can act on.
 *
 * A torrent with no peers is the state a reader is most likely to meet, and
 * "0 B/s" explains nothing. This says who was asked, what answered and what
 * happens next, so a job that is waiting looks like waiting and not like a
 * broken program.
 */
function Swarm({ job }: { job: TorrentOverviewProps["job"] }) {
  const answered = job.trackers.filter(
    (tracker) => tracker.state === "ok",
  ).length;
  const connected = job.peers.filter((peer) => peer.connected);
  const fastest = [...connected]
    .sort((left, right) => right.downSpeed - left.downSpeed)
    .slice(0, 4);

  if (job.peers.length === 0) {
    return (
      <p className="mt-2 text-chrome-micro leading-4 text-chrome-muted">
        {job.trackers.length > 0
          ? `${answered} of ${job.trackers.length} ${job.trackers.length === 1 ? "tracker" : "trackers"} answered. `
          : "This torrent names no tracker. "}
        The trackers, the peer table and the local network are asked again in a
        minute. A torrent nobody seeds stays at zero, so leave it running.
      </p>
    );
  }

  if (fastest.length === 0) {
    return (
      <p className="mt-2 text-chrome-micro text-chrome-muted">
        {job.peers.length} {job.peers.length === 1 ? "peer is" : "peers are"}{" "}
        known but none is connected yet; each one is tried in turn.
      </p>
    );
  }

  return (
    <ul className="mt-2 flex flex-col gap-0.5">
      {fastest.map((peer) => (
        <li
          key={peer.address}
          className="flex items-baseline justify-between gap-3 text-chrome-micro"
        >
          <span className="min-w-0 truncate tabular-nums text-chrome-menu-text">
            {peer.address}
          </span>
          <span className="shrink-0 tabular-nums text-chrome-muted">
            {peer.downSpeed > 0 ? formatSpeed(peer.downSpeed) : "idle"}
          </span>
        </li>
      ))}
      {connected.length > fastest.length ? (
        <li className="text-chrome-micro text-chrome-muted">
          and {connected.length - fastest.length} more connected
        </li>
      ) : null}
    </ul>
  );
}

/**
 * The summary page of a job.
 *
 * Two columns, both of them carrying information: the shape of the torrent on
 * the left — its pieces and who is serving them — and the numbers on the
 * right. The pane is filled top to bottom, so a job with a small file does
 * not leave half the window blank under a handful of rows.
 */
export function TorrentOverview({ job }: TorrentOverviewProps) {
  const count = job.pieceMap.count;
  const have = job.pieceMap.have;
  const onDisk = formatSize(job.downloaded);
  const connected = job.peers.filter((peer) => peer.connected).length;
  const answered = job.trackers.filter(
    (tracker) => tracker.state === "ok",
  ).length;

  const files =
    job.files.length === 0
      ? "Not known yet"
      : job.selected.length > 0 && job.selected.length < job.files.length
        ? `${job.selected.length} of ${job.files.length} selected`
        : `${job.files.length}`;

  const coming =
    job.pieceInflight === null
      ? ""
      : ` Piece ${job.pieceInflight + 1} is coming in.`;
  const gridLabel = `${have} of ${count} pieces on disk, ${onDisk}.${coming}`;

  return (
    <div className="grid min-h-full grid-cols-[minmax(0,1fr)_14rem] grid-rows-[minmax(0,1fr)] gap-6 overflow-hidden pb-3">
      <div className="flex min-h-0 min-w-0 flex-col gap-4">
        <section className="flex min-h-0 flex-1 flex-col">
          <div className="flex items-baseline justify-between gap-3">
            <p className="text-chrome-micro text-chrome-muted">Pieces</p>
            <p className="text-chrome-micro tabular-nums text-chrome-menu-text">
              {count > 0 ? `${have} of ${count} · ${onDisk}` : "Not known yet"}
            </p>
          </div>
          {count > 0 ? (
            <>
              <div className="mt-2 min-h-0 flex-1">
                <PieceGrid
                  bits={job.pieceBits}
                  count={count}
                  inflight={job.pieceInflight}
                  label={gridLabel}
                  fill
                />
              </div>
              <Legend />
            </>
          ) : (
            <p className="mt-1 text-chrome-micro text-chrome-muted">
              A magnet learns the pieces from its peers; the grid fills in as
              soon as one answers.
            </p>
          )}
        </section>

        <section className="min-w-0">
          <div className="flex items-baseline justify-between gap-3">
            <p className="text-chrome-micro text-chrome-muted">Peers</p>
            <p className="text-chrome-micro tabular-nums text-chrome-muted">
              {connected} connected · {job.peers.length} known
            </p>
          </div>
          <Swarm job={job} />
        </section>

        {job.files.length > 0 ? (
          <section className="min-w-0">
            <div className="flex items-baseline justify-between gap-3">
              <p className="text-chrome-micro text-chrome-muted">Files</p>
              <p className="text-chrome-micro tabular-nums text-chrome-muted">
                {job.files.length} · {formatSize(job.totalLength)}
              </p>
            </div>
            <ul className="mt-2 flex flex-col gap-0.5">
              {job.files.slice(0, 6).map((file) => (
                <li
                  key={file.path}
                  className="flex items-baseline justify-between gap-3 text-chrome-micro"
                >
                  <Tooltip content={file.path} className="min-w-0">
                    <span className="block truncate text-chrome-menu-text">
                      {file.path}
                    </span>
                  </Tooltip>
                  <span className="shrink-0 tabular-nums text-chrome-muted">
                    {formatSize(file.length)}
                  </span>
                </li>
              ))}
              {job.files.length > 6 ? (
                <li className="text-chrome-micro text-chrome-muted">
                  and {job.files.length - 6} more on the Files page
                </li>
              ) : null}
            </ul>
          </section>
        ) : null}
      </div>

      <dl className="flex min-w-0 flex-col overflow-hidden">
        <Row label="Size" value={formatSize(job.totalLength)} />
        <Row label="Downloaded" value={formatSize(job.downloaded)} />
        <Row label="Speed" value={formatSpeed(job.downSpeed) || "0 B/s"} />
        <Row label="Uploaded" value={formatSpeed(job.upSpeed) || "0 B/s"} />
        <Row
          label="Time left"
          value={job.eta === null ? "—" : formatEta(job.eta)}
        />
        <Row label="Files" value={files} />
        <Row
          label="Trackers"
          value={`${answered} of ${job.trackers.length} answering`}
        />
        <Row label="Saved in" value={job.directory} title={job.directory} />
      </dl>
    </div>
  );
}
