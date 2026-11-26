import { useState } from "react";

import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import {
  torrentAddTrackers,
  torrentAnnounceNow,
  torrentRemoveTrackers,
  type TorrentJob,
  type TorrentTracker,
} from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

import { AddTrackersDialog } from "./add-trackers-dialog";

interface TorrentTrackersProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
  /** Do something to the job and read the jobs again. */
  run: (action: () => Promise<unknown>) => Promise<void>;
}

/** The head of a table of the window: small, quiet and sticky. */
const HEAD =
  "sticky top-0 z-10 bg-chrome-popup pb-1.5 text-chrome-micro font-normal text-chrome-muted";

/** The short name of a tracker: its host, without the scheme or the path. */
function trackerHost(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url.replace(/^[a-z+]+:\/\//i, "").split("/")[0] || url;
  }
}

/** The words a tracker's state is shown with. The engine's own message is
 * long and technical, so it stays in the tooltip. */
function trackerWords(tracker: TorrentTracker): string {
  switch (tracker.state) {
    case "working":
      return "Asking…";
    case "ok":
      return tracker.peers > 0 ? "Answered" : "No peers";
    case "error":
      return "No answer";
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
 * The trackers of a job, with the controls a torrent client has: add one,
 * check them now, drop one.
 *
 * A tracker that answers is asked again after the time it gave; one that
 * cannot be reached says so in the error tone, because a tracker that is down
 * is the usual reason a torrent finds nobody. Adding is the fix a reader
 * reaches for when a torrent sits at zero, so it sits at the top of the page
 * with the reason written under it, in plain words.
 */
export function TorrentTrackers({ job, run }: TorrentTrackersProps) {
  const [adding, setAdding] = useState(false);
  const answering = job.trackers.filter(
    (tracker) => tracker.state === "ok",
  ).length;

  return (
    <div className="flex min-h-full flex-col pb-2">
      <div className="flex flex-wrap items-center justify-between gap-2 pb-2">
        <p className="text-chrome-micro tabular-nums text-chrome-muted">
          {job.trackers.length === 0
            ? "No trackers"
            : `${answering} of ${job.trackers.length} answering`}
        </p>
        <div className="flex items-center gap-1.5">
          <DialogAction tone="secondary" onClick={() => setAdding(true)}>
            Add trackers…
          </DialogAction>
          <DialogAction
            tone="quiet"
            onClick={() => void run(() => torrentAnnounceNow(job.infoHash))}
          >
            Check now
          </DialogAction>
        </div>
      </div>

      <p className="pb-3 text-chrome-micro leading-4 text-chrome-muted">
        Trackers are servers that tell the app who else has this torrent. A
        torrent stuck at zero usually needs one more from its page.
      </p>

      {job.trackers.length === 0 ? (
        <p className="py-6 text-center text-xs text-chrome-muted">
          This torrent names no tracker. Its peers come from the peer table and
          the local network, and a tracker added here is asked at once.
        </p>
      ) : (
        <table className="w-full table-fixed border-collapse text-xs">
          <thead>
            <tr className="text-left">
              <th scope="col" className={`${HEAD} pl-1`}>
                Tracker
              </th>
              <th scope="col" className={`${HEAD} w-32`}>
                Status
              </th>
              <th scope="col" className={`${HEAD} w-24 text-right`}>
                Seeders / leechers
              </th>
              <th scope="col" className={`${HEAD} w-16 text-right`}>
                Again in
              </th>
              <th scope="col" className={`${HEAD} w-8 pr-2`}>
                <span className="sr-only">Remove</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {job.trackers.map((tracker) => (
              <tr
                key={tracker.url}
                className="odd:hover:bg-chrome-popup-hover even:bg-chrome-row-alt even:hover:bg-chrome-popup-hover"
              >
                <td className="py-1.5 pr-3 pl-1">
                  <Tooltip content={tracker.url} className="w-full">
                    <span className="block truncate text-chrome-menu-text">
                      {trackerHost(tracker.url)}
                    </span>
                  </Tooltip>
                </td>
                <td className="w-32 py-1.5 pr-3">
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
                <td className="w-24 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                  {tracker.seeders} / {tracker.leechers}
                </td>
                <td className="w-16 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                  {tracker.nextIn === null ? "—" : `${tracker.nextIn}s`}
                </td>
                <td className="w-8 py-1.5 pr-2 text-right">
                  <Tooltip content="Remove this tracker" side="left">
                    <button
                      type="button"
                      aria-label={`Remove ${trackerHost(tracker.url)}`}
                      onClick={() =>
                        void run(() =>
                          torrentRemoveTrackers(job.infoHash, [tracker.url]),
                        )
                      }
                      className="text-chrome-muted transition-colors hover:text-chrome-error focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none"
                    >
                      ×
                    </button>
                  </Tooltip>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <AddTrackersDialog
        open={adding}
        onClose={() => setAdding(false)}
        jobId={job.infoHash}
        onAdd={(urls) =>
          run(() => torrentAddTrackers(job.infoHash, urls))
        }
      />
    </div>
  );
}
