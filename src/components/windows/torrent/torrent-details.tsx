import { useState } from "react";

import { DialogAction } from "@/components/ui/dialog-action";
import { Progress } from "@/components/ui/progress";
import { formatEta, formatSize, formatSpeed } from "@/lib/downloads";
import { openPath } from "@/lib/ipc";
import {
  torrentPause,
  torrentRecheck,
  torrentRemove,
  torrentResume,
  torrentSetSeeding,
  type TorrentJob,
} from "@/lib/ipc-torrent";

import { TorrentFiles } from "./torrent-files";
import { TorrentOverview } from "./torrent-overview";
import { TorrentPeers } from "./torrent-peers";
import { TorrentPieces } from "./torrent-pieces";
import { TorrentTabs, type TorrentTabId } from "./torrent-tabs";
import { TorrentTrackers } from "./torrent-trackers";
import { fraction, statusSentence } from "./torrent-status";
import { cn } from "@/lib/utils";

interface TorrentDetailsProps {
  /** The job the reader is looking at. */
  job: TorrentJob | null;
  /** Whether something is being done right now. */
  busy: boolean;
  /** Do something to the job and read the jobs again. */
  run: (action: () => Promise<unknown>) => Promise<void>;
}

/**
 * What a job is: its size, its speed, its peers and what it is doing.
 *
 * The pages are the five the reader asks for: the summary, the files, the
 * pieces, the peers and the trackers.
 */
export function TorrentDetails({ job, busy, run }: TorrentDetailsProps) {
  const [tab, setTab] = useState<TorrentTabId>("overview");
  // A removal is asked for once: the first click arms it, the second one
  // means it, because removing a job that is seeding stops the upload.
  const [removing, setRemoving] = useState<"job" | "files" | null>(null);

  if (!job) {
    return (
      <div className="flex flex-1 items-center justify-center px-6">
        <p className="max-w-64 text-center text-xs text-chrome-muted">
          Pick a torrent to see its files, its pieces, its peers and what it
          is doing.
        </p>
      </div>
    );
  }

  const paused = job.status.state === "paused";
  const done = job.status.state === "completed" || job.status.state === "seeding";
  const seeding = job.status.state === "seeding";

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="shrink-0 px-4 pt-3 pb-2">
        <div className="flex items-baseline justify-between gap-3">
          <p className="truncate text-sm font-medium text-chrome-title-text">
            {job.name}
          </p>
          <span className="shrink-0 text-xs tabular-nums text-chrome-menu-text">
            {Math.round(fraction(job) * 100)}%
          </span>
        </div>
        <p
          className={cn(
            "mt-0.5 truncate text-chrome-micro",
            job.error || job.status.state === "failed"
              ? "text-chrome-error"
              : "text-chrome-muted",
          )}
        >
          {statusSentence(job.status)}
        </p>
        <Progress
          value={fraction(job) * 100}
          aria-label={`Progress of ${job.name}`}
          className="mt-2 h-1.5"
        />
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 pt-2 text-chrome-micro tabular-nums text-chrome-muted">
          <span>
            {formatSize(job.downloaded)} of {formatSize(job.totalLength)}
          </span>
          <span>{formatSpeed(job.downSpeed) || "0 B/s"} down</span>
          <span>{formatSpeed(job.upSpeed) || "0 B/s"} up</span>
          <span>
            {job.eta === null ? "no estimate" : `${formatEta(job.eta)} left`}
          </span>
        </div>
      </div>

      <div className="shrink-0 px-4 pb-2">
        <TorrentTabs active={tab} onChange={setTab} />
      </div>

      <div
        role="tabpanel"
        id={`torrent-panel-${tab}`}
        aria-labelledby={`torrent-tab-${tab}`}
        className="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto scrollbar-thin px-4"
      >
        {tab === "overview" ? <TorrentOverview job={job} /> : null}
        {tab === "files" ? <TorrentFiles job={job} /> : null}
        {tab === "pieces" ? <TorrentPieces job={job} /> : null}
        {tab === "peers" ? <TorrentPeers job={job} /> : null}
        {tab === "trackers" ? <TorrentTrackers job={job} /> : null}
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-chrome-line px-4 py-2">
        {paused ? (
          <DialogAction
            tone="primary"
            disabled={busy}
            onClick={() => void run(() => torrentResume(job.infoHash))}
          >
            Resume
          </DialogAction>
        ) : (
          <DialogAction
            tone="primary"
            disabled={busy}
            onClick={() => void run(() => torrentPause(job.infoHash))}
          >
            Pause
          </DialogAction>
        )}
        {done ? (
          <DialogAction
            tone="secondary"
            disabled={busy}
            onClick={() =>
              void run(() => torrentSetSeeding(job.infoHash, !job.seeding))
            }
          >
            {seeding ? "Stop seeding" : "Seed this"}
          </DialogAction>
        ) : null}
        <DialogAction
          tone="quiet"
          disabled={busy}
          onClick={() => void run(() => torrentRecheck(job.infoHash))}
        >
          Check again
        </DialogAction>
        <DialogAction
          tone="secondary"
          onClick={() => {
            void openPath(job.directory).catch(() => undefined);
          }}
        >
          Open folder
        </DialogAction>
        <DialogAction
          tone={removing === "job" ? "danger" : "quiet"}
          disabled={busy}
          className="ml-auto"
          onClick={() => {
            if (removing === "job") {
              setRemoving(null);
              void run(() => torrentRemove(job.infoHash, false));
            } else {
              setRemoving("job");
            }
          }}
        >
          {removing === "job" ? "Remove this job?" : "Remove"}
        </DialogAction>
        <DialogAction
          tone={removing === "files" ? "danger" : "quiet"}
          disabled={busy}
          onClick={() => {
            if (removing === "files") {
              setRemoving(null);
              void run(() => torrentRemove(job.infoHash, true));
            } else {
              setRemoving("files");
            }
          }}
        >
          {removing === "files" ? "Remove the files too?" : "Remove with files"}
        </DialogAction>
      </div>
    </div>
  );
}
