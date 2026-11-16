import { Tooltip } from "@/components/ui/tooltip";
import type { TorrentJob, TorrentTracker } from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

interface TorrentTrackersProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/** The words a tracker's state is shown with. The engine's own message is
 * long and technical, so it stays in the tooltip. */
function trackerWords(tracker: TorrentTracker): string {
  switch (tracker.state) {
    case "working":
      return "Asking";
    case "ok":
      return tracker.peers > 0 ? `${tracker.peers} peers` : "No peers";
    case "error":
      return "Could not connect";
  }
}

/** What the state means, in a sentence. */
function trackerDetail(tracker: TorrentTracker): string {
  switch (tracker.state) {
    case "working":
      return "The tracker is being asked who has this torrent.";
    case "ok":
      return tracker.peers > 0
        ? `The tracker named ${tracker.peers} ${tracker.peers === 1 ? "peer" : "peers"}.`
        : "The tracker answered but named no peers.";
    case "error":
      return tracker.message || "The tracker did not answer.";
  }
}

/**
 * The trackers of a job: the servers that say who has the torrent.
 *
 * A tracker that answers is asked again after the time it gave; one that
 * cannot be reached says so in the error colour, because a tracker that is
 * down is the usual reason a torrent finds nobody.
 */
export function TorrentTrackers({ job }: TorrentTrackersProps) {
  if (job.trackers.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-chrome-muted">
        This torrent names no tracker; its peers come from the distributed
        table and the local network instead.
      </p>
    );
  }

  return (
    <div className="pb-2">
      <p className="mb-2 text-chrome-micro text-chrome-muted">
        Trackers are the servers that tell this job who has its files.
      </p>
      <table className="w-full table-fixed border-collapse text-xs">
        <thead>
          <tr className="text-left text-chrome-micro text-chrome-muted">
            <th scope="col" className="pb-1 font-normal">Tracker</th>
            <th scope="col" className="w-40 pb-1 font-normal">Status</th>
            <th scope="col" className="w-28 pb-1 text-right font-normal">
              Seeders / leechers
            </th>
            <th scope="col" className="w-16 pb-1 text-right font-normal">
              Ask again
            </th>
          </tr>
        </thead>
        <tbody>
          {job.trackers.map((tracker) => (
            <tr
              key={tracker.url}
              className="border-b border-chrome-line/60 last:border-b-0"
            >
              <td className="py-1.5 pr-3">
                <Tooltip content={tracker.url} className="w-full">
                  <span className="block truncate text-chrome-menu-text">
                    {tracker.url}
                  </span>
                </Tooltip>
              </td>
              <td className="w-40 py-1.5 pr-3">
                <Tooltip content={trackerDetail(tracker)} className="w-full">
                  <span
                    className={cn(
                      "block truncate text-chrome-micro",
                      tracker.state === "error"
                        ? "text-chrome-error"
                        : "text-chrome-muted",
                    )}
                  >
                    {trackerWords(tracker)}
                  </span>
                </Tooltip>
              </td>
              <td className="w-28 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {tracker.seeders} / {tracker.leechers}
              </td>
              <td className="w-16 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {tracker.nextIn === null ? "—" : `${tracker.nextIn}s`}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
