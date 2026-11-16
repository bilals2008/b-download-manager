import { Progress } from "@/components/ui/progress";
import { formatSize, formatSpeed } from "@/lib/downloads";
import { cn } from "@/lib/utils";

import { fraction, statusSentence } from "./torrent-status";
import type { TorrentJob } from "@/lib/ipc-torrent";

interface TorrentJobListProps {
  /** Every job of the session. */
  jobs: readonly TorrentJob[];
  /** The job the reader is looking at. */
  chosen: string | null;
  /** Look at another job. */
  onSelect: (id: string) => void;
}

/**
 * The jobs of the session, one after another.
 *
 * The list is the way into a job: the name says what it is, the bar says how
 * far it is, and the line under it says what it is doing now.
 */
export function TorrentJobList({
  jobs,
  chosen,
  onSelect,
}: TorrentJobListProps) {
  if (jobs.length === 0) {
    return (
      <p className="px-3 py-6 text-center text-xs text-chrome-muted">
        No torrents yet. Add one from Add torrent.
      </p>
    );
  }

  return (
    <ul className="flex flex-col">
      {jobs.map((job) => {
        const selected = job.infoHash === chosen;
        return (
          <li key={job.infoHash}>
            <button
              type="button"
              onClick={() => onSelect(job.infoHash)}
              aria-current={selected}
              className={cn(
                "flex w-full flex-col gap-1.5 border-l-2 px-3 py-2.5 text-left transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none",
                selected
                  ? "border-primary bg-chrome-menu-active"
                  : "border-transparent hover:bg-chrome-menu-hover",
              )}
            >
              <span className="flex items-baseline justify-between gap-2">
                <span
                  className={cn(
                    "truncate text-xs text-chrome-title-text",
                    selected && "font-medium",
                  )}
                >
                  {job.name}
                </span>
                <span className="shrink-0 text-chrome-micro tabular-nums text-chrome-muted">
                  {job.totalLength > 0
                    ? `${Math.round(fraction(job) * 100)}%`
                    : ""}
                </span>
              </span>
              <span
                className={cn(
                  "truncate text-chrome-micro",
                  job.error || job.status.state === "failed"
                    ? "text-chrome-error"
                    : "text-chrome-muted",
                )}
              >
                {statusSentence(job.status)}
              </span>
              <Progress
                value={fraction(job) * 100}
                aria-label={`Progress of ${job.name}`}
                className="h-1"
              />
              <span className="flex items-center justify-between gap-2 text-chrome-micro tabular-nums text-chrome-muted">
                <span className="truncate">
                  {formatSize(job.downloaded)} of {formatSize(job.totalLength)}
                </span>
                {job.downSpeed > 0 ? (
                  <span className="shrink-0">{formatSpeed(job.downSpeed)}</span>
                ) : null}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
