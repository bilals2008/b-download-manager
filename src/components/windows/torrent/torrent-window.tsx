import { useEffect, useMemo, useState } from "react";

import { CloseGlyph, SidebarGlyph } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import { formatSpeed } from "@/lib/downloads";
import { torrentPauseAll, torrentResumeAll } from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";

import { TorrentDetails } from "./torrent-details";
import { TorrentJobList } from "./torrent-job-list";
import { useTorrentJobs } from "./use-torrent-jobs";

const LIST_ID = "torrent-job-list";

/**
 * The torrent window.
 *
 * One window holds every job: the jobs on the left, and the summary, files,
 * pieces, peers and trackers of the chosen one on the right. The two sides are
 * told apart by their surface instead of by a rule, so the window reads as one
 * sheet with a darker rail.
 *
 * The rail folds away, so a reader watching one job can give it the whole
 * window; the header keeps the way back.
 */
export function TorrentWindow() {
  const { jobs, job, chosen, select, run, busy, problem } = useTorrentJobs();
  const [railOpen, setRailOpen] = useState(true);

  useEffect(() => {
    void showWindowWhenPainted();
  }, []);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape" && !event.defaultPrevented) {
        void closeWindow();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // What the whole session is doing, in one line, so the header answers the
  // first question a reader has without a job being chosen.
  const totals = useMemo(() => {
    let down = 0;
    let up = 0;
    for (const entry of jobs) {
      down += entry.downSpeed;
      up += entry.upSpeed;
    }
    return { down, up };
  }, [jobs]);

  const summary =
    jobs.length === 0
      ? "Nothing added yet"
      : [
          `${jobs.length} ${jobs.length === 1 ? "torrent" : "torrents"}`,
          formatSpeed(totals.down) || null,
          formatSpeed(totals.up) ? `${formatSpeed(totals.up)} up` : null,
        ]
          .filter(Boolean)
          .join(" · ");

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-popup text-chrome-menu-text">
      <header className="flex shrink-0 items-center gap-2 px-3 pt-3 pb-2">
        <Tooltip content={railOpen ? "Hide the torrent list" : "Show the torrent list"}>
          <button
            type="button"
            onClick={() => setRailOpen((open) => !open)}
            aria-expanded={railOpen}
            aria-controls={LIST_ID}
            aria-label={railOpen ? "Hide the torrent list" : "Show the torrent list"}
            className={cn(
              "flex size-7 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
              railOpen && "bg-chrome-menu-hover text-chrome-menu-text",
            )}
          >
            <SidebarGlyph className="size-4" />
          </button>
        </Tooltip>
        <div data-tauri-drag-region className="min-w-0 flex-1 px-1">
          <p className="truncate text-sm font-medium text-chrome-title-text">
            Torrents
          </p>
          <p className="truncate text-chrome-micro tabular-nums text-chrome-muted">
            {summary}
          </p>
        </div>
        <DialogAction
          tone="quiet"
          disabled={busy || jobs.length === 0}
          onClick={() => void run(() => torrentPauseAll())}
        >
          Pause all
        </DialogAction>
        <DialogAction
          tone="quiet"
          disabled={busy || jobs.length === 0}
          onClick={() => void run(() => torrentResumeAll())}
        >
          Resume all
        </DialogAction>
        <Tooltip content="Close">
          <button
            type="button"
            onClick={() => void closeWindow()}
            aria-label="Close"
            className="flex size-7 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <CloseGlyph className="size-3.5" />
          </button>
        </Tooltip>
      </header>

      {problem ? (
        <p
          role="alert"
          className="shrink-0 px-4 pb-2 text-chrome-micro break-words text-chrome-error"
        >
          {problem}
        </p>
      ) : null}

      <div className="flex min-h-0 flex-1 gap-1 px-1 pb-1">
        <aside
          id={LIST_ID}
          className={cn(
            "w-64 shrink-0 flex-col overflow-y-auto rounded-chrome bg-chrome-menu scrollbar-thin",
            railOpen ? "flex" : "hidden",
          )}
        >
          <TorrentJobList jobs={jobs} chosen={chosen} onSelect={select} />
        </aside>
        <TorrentDetails job={job} busy={busy} run={run} />
      </div>
    </div>
  );
}
