import { useEffect, useRef, useState } from "react";

import { DialogAction } from "@/components/ui/dialog-action";
import { Progress } from "@/components/ui/progress";
import { ProgressWindowTitle } from "@/components/windows/progress/progress-window-title";
import { formatEta, formatSize, formatSpeed } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import {
  mediaCancel,
  mediaJob,
  mediaResume,
  type MediaJobView,
} from "@/lib/media";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";

/** How often the window reads the job's progress. */
const TICK = 1000;

/** One fact of the job, with its name and its value. */
function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-0.5">
      <span className="text-chrome-micro text-chrome-muted">{label}</span>
      <span className="min-w-0 truncate text-xs tabular-nums text-chrome-menu-text">
        {value}
      </span>
    </div>
  );
}

/**
 * A video download in the progress window.
 *
 * The media job has no event stream of its own, so the window reads it once a
 * second: the same facts the engine window shows — size, bytes, rate, time
 * left, resume, server — and the same two controls a stopped job needs.
 */
export function MediaProgressWindow({ job }: { job: number }) {
  const [view, setView] = useState<MediaJobView | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [speed, setSpeed] = useState(0);
  const previous = useRef<{ bytes: number; at: number } | null>(null);

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

  useEffect(() => {
    let live = true;
    const read = () => {
      void mediaJob(job)
        .then((next) => {
          if (!live) {
            return;
          }
          const now = Date.now();
          const before = previous.current;
          if (before && now > before.at) {
            const rate = Math.max(
              0,
              Math.round(((next.downloaded - before.bytes) * 1000) / (now - before.at)),
            );
            setSpeed(next.downloading ? rate : 0);
          }
          previous.current = { bytes: next.downloaded, at: now };
          setView(next);
          setProblem(null);
        })
        .catch((failure: unknown) => {
          if (live) {
            setProblem(describeError(failure));
          }
        });
    };
    read();
    const timer = window.setInterval(read, TICK);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [job]);

  async function act(action: "cancel" | "resume") {
    setBusy(true);
    setProblem(null);
    try {
      if (action === "cancel") {
        await mediaCancel(job);
        await closeWindow();
      } else {
        setView(await mediaResume(job));
      }
    } catch (failure) {
      setProblem(describeError(failure));
    } finally {
      setBusy(false);
    }
  }

  const name = view?.title || "Video download";
  const total = view?.total ?? 0;
  const downloaded = view?.downloaded ?? 0;
  const fraction =
    view && view.total > 0
      ? view.fraction > 0
        ? view.fraction
        : downloaded / total
      : 0;
  const remaining = Math.max(0, total - downloaded);
  const eta = speed > 0 && remaining > 0 ? Math.ceil(remaining / speed) : null;
  const host = (() => {
    try {
      return view ? new URL(view.pageUrl).hostname : "";
    } catch {
      return "";
    }
  })();

  return (
    <div className="flex h-dvh flex-col overflow-hidden rounded-lg bg-chrome-popup text-chrome-menu-text">
      <ProgressWindowTitle name={name} path={view?.dest ?? view?.pageUrl ?? ""} />

      <div className="flex min-h-0 flex-1 flex-col px-4 pt-3 pb-3">
        <div className="flex items-baseline justify-between pb-1">
          <span className="text-xs font-medium text-chrome-title-text">
            {view?.done ? "Saved" : view?.downloading ? "Downloading" : view?.canResume ? "Stopped" : "Video"}
          </span>
          <span className="text-sm font-medium tabular-nums">
            {(fraction * 100).toFixed(2)}%
          </span>
        </div>
        <Progress value={fraction * 100} aria-label={`Progress of ${name}`} className="h-1.5" />
        {view?.stage ? (
          <p className="truncate pt-1 text-chrome-micro text-chrome-muted">{view.stage}</p>
        ) : null}

        <div className="pt-3">
          <Fact label="File size" value={total > 0 ? formatSize(total) : "—"} />
          <Fact
            label="Downloaded"
            value={
              total > 0
                ? `${formatSize(downloaded)} (${fractionPercentage(downloaded, total)})`
                : formatSize(downloaded)
            }
          />
          <Fact
            label="Transfer rate"
            value={view?.downloading && speed > 0 ? formatSpeed(speed) : "—"}
          />
          <Fact label="Time left" value={eta === null ? "—" : formatEta(eta)} />
          <Fact label="Resume capability" value={view?.canResume ? "Yes" : "No"} />
          <Fact label="Speed limit" value="No limit" />
          <Fact label="Server" value={host || "—"} />
        </div>

        {problem || (view?.error && !view.downloading) ? (
          <p role="alert" className="pt-2 text-chrome-micro break-words text-chrome-error">
            {problem || view?.error}
          </p>
        ) : null}

        <div className="mt-auto flex items-center justify-end gap-2 pt-3">
          <DialogAction tone="quiet" disabled={busy} onClick={() => void act("cancel")}>
            Cancel
          </DialogAction>
          {view?.canResume && !view.downloading ? (
            <DialogAction tone="primary" disabled={busy} onClick={() => void act("resume")}>
              Resume
            </DialogAction>
          ) : null}
          <DialogAction tone="quiet" onClick={() => void closeWindow()}>
            Close
          </DialogAction>
        </div>
      </div>
    </div>
  );
}

/** The part of the whole, as the percentage the window shows on the facts. */
function fractionPercentage(downloaded: number, total: number): string {
  if (total <= 0) {
    return "0%";
  }
  return `${((downloaded / total) * 100).toFixed(1)}%`;
}