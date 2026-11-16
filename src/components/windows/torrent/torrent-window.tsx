import { useEffect, useState } from "react";

import { CloseGlyph, SidebarGlyph } from "@/components/icons/ui-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { DialogAction } from "@/components/ui/dialog-action";
import {
  torrentPauseAll,
  torrentResumeAll,
} from "@/lib/ipc-torrent";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";
import { cn } from "@/lib/utils";

import { TorrentDetails } from "./torrent-details";
import { TorrentJobList } from "./torrent-job-list";
import { useTorrentJobs } from "./use-torrent-jobs";

const LIST_ID = "torrent-job-list";

/**
 * The torrent window.
 *
 * One window shows every job: the list of them on the left, and the files,
 * the peers and the trackers of the chosen one on the right. The files are
 * written into a folder named after the torrent, and seeding starts only
 * when the reader turned it on.
 *
 * The list folds away, so a reader looking at one job can give it the whole
 * window; the header keeps the way back.
 */
export function TorrentWindow() {
  const { jobs, job, chosen, select, run, busy, problem } = useTorrentJobs();
  const [listOpen, setListOpen] = useState(true);

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

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-popup text-chrome-menu-text">
      <header className="flex shrink-0 items-start gap-2 px-4 pt-3 pb-2">
        <Tooltip content={listOpen ? "Hide the job list" : "Show the job list"}>
          <button
            type="button"
            onClick={() => setListOpen((open) => !open)}
            aria-expanded={listOpen}
            aria-controls={LIST_ID}
            aria-label={listOpen ? "Hide the job list" : "Show the job list"}
            className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <SidebarGlyph className="size-4" />
          </button>
        </Tooltip>
        <div data-tauri-drag-region className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">Torrents</p>
          <p className="text-chrome-micro text-chrome-muted">
            {jobs.length === 0
              ? "Files, peers and what the job is doing."
              : `${jobs.length} ${jobs.length === 1 ? "job" : "jobs"}`}
          </p>
        </div>
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
        <p role="alert" className="shrink-0 px-4 pb-2 text-chrome-micro text-chrome-error">
          {problem}
        </p>
      ) : null}

      <div className="flex min-h-0 flex-1 border-t border-chrome-line">
        <aside
          id={LIST_ID}
          className={cn(
            "w-60 shrink-0 flex-col overflow-y-auto border-r border-chrome-line scrollbar-thin",
            listOpen ? "flex" : "hidden",
          )}
        >
          <TorrentJobList jobs={jobs} chosen={chosen} onSelect={select} />
        </aside>
        <TorrentDetails job={job} busy={busy} run={run} />
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-chrome-line px-4 py-2">
        <DialogAction
          tone="secondary"
          disabled={busy || jobs.length === 0}
          onClick={() => void run(() => torrentPauseAll())}
        >
          Pause all
        </DialogAction>
        <DialogAction
          tone="secondary"
          disabled={busy || jobs.length === 0}
          onClick={() => void run(() => torrentResumeAll())}
        >
          Resume all
        </DialogAction>
      </footer>
    </div>
  );
}
