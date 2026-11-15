import { useEffect, useState } from "react";

import { CloseGlyph } from "@/components/icons/ui-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { Progress } from "@/components/ui/progress";
import { DialogAction } from "@/components/ui/dialog-action";
import { formatSize } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import {
  torrentJobs,
  torrentPause,
  torrentRemove,
  torrentResume,
  torrentSetSeeding,
  type TorrentJob,
} from "@/lib/ipc-torrent";
import { openPath } from "@/lib/ipc";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";

/** The words a job shows for what it is doing. */
function statusLine(job: TorrentJob): string {
  switch (job.status.state) {
    case "idle":
      return "Not started yet.";
    case "looking":
      return "Looking for peers.";
    case "noPeers":
      return "No peers yet.";
    case "downloading":
      return "Downloading.";
    case "paused":
      return "Paused.";
    case "completed":
      return "Finished.";
    case "seeding":
      return "Seeding — uploading to the swarm.";
    case "failed":
      return job.status.message || "This torrent could not be downloaded.";
  }
}

/** The percent of the job that is on disk. */
function fraction(job: TorrentJob): number {
  return job.totalLength > 0 ? job.downloaded / job.totalLength : 0;
}

/**
 * The torrent window.
 *
 * One window shows every job: the files, the peers and what the job is
 * doing. The files are written into a folder named after the torrent, and
 * seeding starts only when the reader turned it on.
 */
export function TorrentWindow() {
  const [jobs, setJobs] = useState<TorrentJob[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // The job the reader asked to remove; it takes a second click to mean it,
  // because a remove while seeding stops the upload.
  const [askedToRemove, setAskedToRemove] = useState<string | null>(null);

  useEffect(() => {
    void showWindowWhenPainted();
    if (!isDesktop()) {
      return;
    }
    let active = true;
    const read = () => {
      void torrentJobs()
        .then((list) => {
          if (active) {
            setJobs(list);
            setProblem(null);
          }
        })
        .catch(() => {
          // The window keeps the last picture rather than flashing empty.
        });
    };
    read();
    const timer = window.setInterval(read, 1000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
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

  function ask(action: () => Promise<void>) {
    if (busy) {
      return;
    }
    setBusy(true);
    setProblem(null);
    void action()
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      })
      .finally(() => {
        setBusy(false);
        void torrentJobs().then(setJobs).catch(() => undefined);
      });
  }

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-popup text-chrome-menu-text">
      <header
        data-tauri-drag-region
        className="flex shrink-0 items-start justify-between gap-3 px-4 pt-3 pb-2"
      >
        <div data-tauri-drag-region className="min-w-0">
          <p className="truncate text-sm font-medium">Torrents</p>
          <p className="text-chrome-micro text-chrome-muted">
            Files, peers and what the job is doing.
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

      <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 pb-4">
        {jobs.length === 0 ? (
          <p className="pt-8 text-center text-xs text-chrome-muted">
            Nothing is running. Paste a magnet link in Add URL to start one.
          </p>
        ) : (
          jobs.map((job) => {
            const percent = Math.min(100, Math.max(0, fraction(job) * 100));
            const status = job.status.state;
            const paused = status === "paused";
            const done = status === "completed" || status === "seeding";
            const seeding = job.seeding && status === "seeding";
            return (
              <section
                key={job.infoHash}
                className="flex flex-col gap-1.5 rounded-chrome bg-chrome-canvas p-3"
              >
                <p className="truncate text-xs text-chrome-title-text">{job.name}</p>
                <p className="text-chrome-micro text-chrome-muted">
                  {statusLine(job)}
                  {done && job.totalLength > 0
                    ? ` ${formatSize(job.downloaded)} in ${job.totalLength} bytes`
                    : null}
                </p>
                {!done ? (
                  <Progress value={percent} aria-label={`Progress of ${job.name}`} />
                ) : null}
                <p className="text-chrome-micro tabular-nums text-chrome-muted">
                  {formatSize(job.downloaded)} of {formatSize(job.totalLength)}
                  {" · "}
                  {job.peers.length} {job.peers.length === 1 ? "peer" : "peers"}
                </p>
                {done ? null : (
                  <ul className="max-h-24 overflow-y-auto scrollbar-thin">
                    {job.files.map((file) => (
                      <li
                        key={file.path}
                        className="flex items-baseline justify-between gap-2 py-px text-chrome-micro text-chrome-menu-text"
                      >
                        <span className="min-w-0 truncate">{file.path}</span>
                        <span className="shrink-0 tabular-nums text-chrome-muted">
                          {formatSize(file.length)}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  {paused ? (
                    <DialogAction
                      tone="quiet"
                      disabled={busy}
                      onClick={() =>
                        ask(() => torrentResume(job.infoHash).then(() => undefined))
                      }
                    >
                      Resume
                    </DialogAction>
                  ) : done ? null : (
                    <DialogAction
                      tone="quiet"
                      disabled={busy}
                      onClick={() =>
                        ask(() => torrentPause(job.infoHash).then(() => undefined))
                      }
                    >
                      Pause
                    </DialogAction>
                  )}
                  {done ? (
                    <>
                      <DialogAction
                        tone="quiet"
                        onClick={() => {
                          void openPath(job.directory).catch(() => undefined);
                        }}
                      >
                        Open folder
                      </DialogAction>
                      <DialogAction
                        tone={seeding ? "primary" : "quiet"}
                        disabled={busy}
                        onClick={() =>
                          ask(() =>
                            torrentSetSeeding(job.infoHash, !job.seeding).then(
                              () => undefined,
                            ),
                          )
                        }
                      >
                        {seeding ? "Stop seeding" : "Seed this"}
                      </DialogAction>
                    </>
                  ) : null}
                  <DialogAction
                    tone={askedToRemove === job.infoHash ? "primary" : "quiet"}
                    disabled={busy}
                    onClick={() => {
                      if (askedToRemove === job.infoHash) {
                        setAskedToRemove(null);
                        void ask(() =>
                          torrentRemove(job.infoHash, false).then(() => undefined),
                        );
                      } else {
                        setAskedToRemove(job.infoHash);
                      }
                    }}
                  >
                    {askedToRemove === job.infoHash
                      ? job.seeding && job.status.state === "seeding"
                        ? "Stop seeding and remove"
                        : "Remove this job?"
                      : "Remove"}
                  </DialogAction>
                </div>
              </section>
            );
          })
        )}
      </main>
    </div>
  );
}

