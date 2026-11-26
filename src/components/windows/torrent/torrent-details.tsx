import { useState } from "react";

import { DialogAction } from "@/components/ui/dialog-action";
import { Progress } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/tooltip";
import { formatSize } from "@/lib/downloads";
import { openPath } from "@/lib/ipc";
import {
  torrentPause,
  torrentRecheck,
  torrentRemove,
  torrentResume,
  torrentSetSeeding,
  type TorrentJob,
} from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

import { TorrentFiles } from "./torrent-files";
import { TorrentOverview } from "./torrent-overview";
import { TorrentPeers } from "./torrent-peers";
import { TorrentPieces } from "./torrent-pieces";
import { TorrentTabs, type TorrentTabId } from "./torrent-tabs";
import { TorrentTrackers } from "./torrent-trackers";
import { fraction, statusSentence } from "./torrent-status";

interface TorrentDetailsProps {
  /** The job the reader is looking at. */
  job: TorrentJob | null;
  /** Whether something is being done right now. */
  busy: boolean;
  /** Do something to the job and read the jobs again. */
  run: (action: () => Promise<unknown>) => Promise<void>;
}

/**
 * What one job is: its progress, its numbers, its files and its swarm.
 *
 * The summary stays above the pages, so the reader never loses the answer to
 * "how is it going" while looking at a tracker. The commands sit below the
 * pages, and removing a job asks once, in place, instead of opening a second
 * question.
 */
export function TorrentDetails({ job, busy, run }: TorrentDetailsProps) {
  const [tab, setTab] = useState<TorrentTabId>("overview");
  const [removing, setRemoving] = useState(false);

  if (!job) {
    return (
      <div className="flex min-w-0 flex-1 flex-col items-center justify-center px-8 text-center">
        <p className="max-w-64 text-xs text-chrome-menu-text">
          No torrent chosen
        </p>
        <p className="mt-1 max-w-64 text-chrome-micro text-chrome-muted">
          Pick one on the left to see its files, pieces, peers and what it is
          doing.
        </p>
      </div>
    );
  }

  const paused = job.status.state === "paused";
  const done = job.status.state === "completed" || job.status.state === "seeding";
  const seeding = job.status.state === "seeding";
  const percent = Math.round(fraction(job) * 100);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col px-4">
      <div className="shrink-0 pt-1 pb-2">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <Tooltip content={job.name} className="block min-w-0">
              <p className="truncate text-[13px] font-medium text-chrome-title-text">
                {job.name}
              </p>
            </Tooltip>
            <p
              className={cn(
                "mt-0.5 truncate text-chrome-micro",
                job.error || job.status.state === "failed"
                  ? "text-chrome-error"
                  : "text-chrome-muted",
              )}
            >
              {statusSentence(job.status)} · {formatSize(job.downloaded)} of{" "}
              {formatSize(job.totalLength)}
            </p>
          </div>
          <p className="shrink-0 text-lg leading-6 tabular-nums text-chrome-title-text">
            {percent}%
          </p>
        </div>
        <Progress
          value={fraction(job) * 100}
          aria-label={`Progress of ${job.name}`}
          className="mt-2 h-1.5"
        />
      </div>

      <div className="shrink-0">
        <TorrentTabs active={tab} onChange={setTab} />
      </div>

      {/* The gutter is reserved even when a page does not scroll, so a table
          never jumps sideways and the bar never sits on the last column. */}
      <div
        role="tabpanel"
        id={`torrent-panel-${tab}`}
        aria-labelledby={`torrent-tab-${tab}`}
        className="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto pt-2 pr-2 scrollbar-thin [scrollbar-gutter:stable]"
      >
        {tab === "overview" ? <TorrentOverview job={job} /> : null}
        {tab === "files" ? <TorrentFiles job={job} /> : null}
        {tab === "pieces" ? <TorrentPieces job={job} /> : null}
        {tab === "peers" ? <TorrentPeers job={job} /> : null}
        {tab === "trackers" ? <TorrentTrackers job={job} run={run} /> : null}
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-2 pt-2 pb-3">
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
            disabled={busy || done}
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
          tone="quiet"
          onClick={() => {
            void openPath(job.directory).catch(() => undefined);
          }}
        >
          Open folder
        </DialogAction>

        {removing ? (
          <div className="ml-auto flex flex-wrap items-center justify-end gap-1.5">
            <span className="mr-1 text-chrome-micro text-chrome-muted">
              Remove this torrent?
            </span>
            <DialogAction
              tone="quiet"
              disabled={busy}
              onClick={() => setRemoving(false)}
            >
              Keep
            </DialogAction>
            <DialogAction
              tone="danger"
              disabled={busy}
              onClick={() => {
                setRemoving(false);
                void run(() => torrentRemove(job.infoHash, false));
              }}
            >
              Keep files
            </DialogAction>
            <DialogAction
              tone="danger"
              disabled={busy}
              onClick={() => {
                setRemoving(false);
                void run(() => torrentRemove(job.infoHash, true));
              }}
            >
              Delete files
            </DialogAction>
          </div>
        ) : (
          <DialogAction
            tone="quiet"
            className="ml-auto"
            disabled={busy}
            onClick={() => setRemoving(true)}
          >
            Remove…
          </DialogAction>
        )}
      </div>
    </div>
  );
}
