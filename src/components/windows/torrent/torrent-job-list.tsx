import { Progress } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/tooltip";
import { formatSize, formatSpeed } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

import { fraction, statusSentence, statusTone } from "./torrent-status";

interface TorrentJobListProps {
  /** Every job of the session. */
  jobs: readonly TorrentJob[];
  /** The job the reader is looking at. */
  chosen: string | null;
  /** Look at another job. */
  onSelect: (id: string) => void;
}

/** The dot that says whether a job is moving, held, done or broken. */
const DOT: Record<string, string> = {
  active: "bg-chrome-focus",
  paused: "bg-chrome-muted",
  done: "bg-chrome-focus",
  failed: "bg-chrome-error",
};

/**
 * The jobs of the session, one after another.
 *
 * Every row says the same four things in the same order: what the torrent is,
 * how far it is, what it is doing and how fast. The chosen row is a surface,
 * not a marker in the margin, so the eye lands on the name first.
 */
export function TorrentJobList({
  jobs,
  chosen,
  onSelect,
}: TorrentJobListProps) {
  if (jobs.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1 px-5 py-8 text-center">
        <p className="text-xs text-chrome-menu-text">No torrents yet</p>
        <p className="text-chrome-micro text-chrome-muted">
          Add one from Tasks › Add torrent, or paste a magnet link into Add URL.
        </p>
      </div>
    );
  }

  return (
    <ul className="flex flex-col gap-0.5 p-1.5">
      {jobs.map((job) => {
        const selected = job.infoHash === chosen;
        const tone = statusTone(job.status);
        const percent = Math.round(fraction(job) * 100);
        return (
          <li key={job.infoHash}>
            <button
              type="button"
              onClick={() => onSelect(job.infoHash)}
              aria-current={selected}
              className={cn(
                "flex w-full flex-col gap-1.5 rounded-chrome px-2.5 py-2 text-left transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
                selected
                  ? "bg-chrome-menu-active"
                  : "hover:bg-chrome-menu-hover",
              )}
            >
              <span className="flex min-w-0 items-center gap-2">
                <span
                  aria-hidden="true"
                  className={cn(
                    "size-1.5 shrink-0 rounded-full",
                    DOT[tone] ?? DOT.active,
                  )}
                />
                <Tooltip content={job.name} className="min-w-0 flex-1">
                  <span className="block truncate text-xs text-chrome-title-text">
                    {job.name}
                  </span>
                </Tooltip>
                <span className="shrink-0 text-chrome-micro tabular-nums text-chrome-muted">
                  {job.totalLength > 0 ? `${percent}%` : ""}
                </span>
              </span>
              <Progress
                value={fraction(job) * 100}
                aria-label={`Progress of ${job.name}`}
                className="h-1"
              />
              <span className="flex min-w-0 items-baseline justify-between gap-2 text-chrome-micro">
                <span
                  className={cn(
                    "min-w-0 truncate",
                    tone === "failed"
                      ? "text-chrome-error"
                      : "text-chrome-muted",
                  )}
                >
                  {statusSentence(job.status)}
                </span>
                <span className="shrink-0 tabular-nums text-chrome-muted">
                  {job.downSpeed > 0
                    ? formatSpeed(job.downSpeed)
                    : formatSize(job.downloaded)}
                </span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
