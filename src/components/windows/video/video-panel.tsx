import { useEffect, useMemo, useRef, useState } from "react";

import { CloseGlyph, DownloadGlyph } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { Progress } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/tooltip";
import { FormatList } from "@/components/windows/video/format-list";
import { HelperOffer } from "@/components/windows/video/helper-offer";
import { useMediaJob } from "@/components/windows/video/use-media-job";
import { formatSize } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { openPath } from "@/lib/ipc";
import {
  formatDuration,
  initialQuality,
  mediaAllowCookies,
  mediaCancel,
  mediaDownload,
  mediaReparse,
  mediaRows,
  videoHelperStatus,
  type VideoHelperStatus,
} from "@/lib/media";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";

/**
 * The quality panel of one video.
 *
 * The window is handed a job number before the first render. Parsing happens
 * in the backend; this page draws the rows it is told about, starts the chosen
 * pair, and follows the download it started. Closing the window only closes
 * the panel: the download keeps going.
 */
export function VideoPanel({ initialJob }: { initialJob: number }) {
  const { view, failure, refresh } = useMediaJob(initialJob);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [consentBusy, setConsentBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [helperStatus, setHelperStatus] = useState<VideoHelperStatus | null>(null);
  const [offerDismissed, setOfferDismissed] = useState(false);
  const selectedFor = useRef<number | null>(null);

  const rows = useMemo(() => (view ? mediaRows(view) : []), [view]);

  // Ask once which helpers are there. A failed check is treated as "both
  // there": a status error must not turn into an install prompt.
  useEffect(() => {
    let cancelled = false;
    videoHelperStatus()
      .then((status) => {
        if (!cancelled) {
          setHelperStatus(status);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setHelperStatus({ ytdlp: true, ffmpeg: true });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const offerHelper = Boolean(
    view &&
      view.state !== "parsing" &&
      helperStatus &&
      (!helperStatus.ytdlp || !helperStatus.ffmpeg) &&
      !offerDismissed,
  );

  // A ready job opens on this host's last choice, else 1080p. Later polls
  // carry the same job and must not reset a choice the reader just made.
  useEffect(() => {
    if (view?.state === "ready" && selectedFor.current !== view.job) {
      selectedFor.current = view.job;
      setSelected(initialQuality(view));
      setActionError(null);
      // A newer video in the same window gets its own chance to set the
      // helpers up.
      setOfferDismissed(false);
    }
  }, [view]);

  useEffect(() => {
    void showWindowWhenPainted();
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape" && !event.defaultPrevented) {
        void closeWindow();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function start(id?: string) {
    const target = id || selected;
    if (!view || !target || busy || view.downloading || view.done) {
      return;
    }
    const row = rows.find((entry) => entry.id === target);
    // The window serves every video, so a click can land after another video
    // replaced this one. A quality that is not in the list on screen is not
    // this job's quality.
    if (!row) {
      setSelected(null);
      setActionError("Choose a quality for this video.");
      return;
    }
    setBusy(true);
    setActionError(null);
    try {
      await mediaDownload(view.job, {
        video: row.id,
        audio: row.audio ?? null,
        // The chosen quality belongs in the name, so two files of the same
        // video do not replace each other.
        name: `${view.title || "video"} ${row.label}`,
      });
      await refresh();
    } catch (error) {
      setActionError(describeError(error));
    } finally {
      setBusy(false);
    }
  }

  async function allowCookies() {
    if (!view) {
      return;
    }
    setConsentBusy(true);
    setActionError(null);
    try {
      await mediaAllowCookies(view.job);
      await refresh();
    } catch (error) {
      setActionError(describeError(error));
    } finally {
      setConsentBusy(false);
    }
  }

  async function afterHelper(status: VideoHelperStatus) {
    setHelperStatus(status);
    if (!view) {
      return;
    }
    try {
      await mediaReparse(view.job);
      await refresh();
    } catch (error) {
      setActionError(describeError(error));
    }
  }

  async function stop() {
    if (!view) {
      return;
    }
    setBusy(true);
    setActionError(null);
    try {
      await mediaCancel(view.job);
      await refresh();
    } catch (error) {
      setActionError(describeError(error));
    } finally {
      setBusy(false);
    }
  }

  const percent = view
    ? Math.min(100, Math.max(0, view.fraction * 100))
    : 0;

  const progressLine = view
    ? view.total > 0
      ? `${formatSize(view.downloaded)} of ${formatSize(view.total)}`
      : `${view.segmentsDone} of ${view.segmentsTotal} segments`
    : "";

  const summary = (() => {
    if (!view) {
      return failure || "Opening…";
    }
    if (view.state === "parsing") {
      return "Reading the manifest…";
    }
    if (view.state === "failed") {
      return "This stream cannot be downloaded";
    }
    if (view.downloading) {
      return view.stage ? `${view.stage} · ${progressLine}` : progressLine;
    }
    if (view.done) {
      return "Saved";
    }
    if (view.stage === "Stopped") {
      return "Stopped · choose Download to try again";
    }
    if (view.stage && view.error) {
      return view.stage;
    }
    const duration = formatDuration(view.duration);
    const count = rows.length === 1 ? "1 quality" : `${rows.length} qualities`;
    return duration ? `${count} · ${duration}` : count;
  })();

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-popup text-chrome-menu-text">
      <header
        data-tauri-drag-region
        className="flex shrink-0 items-start justify-between gap-3 px-4 pt-3 pb-2"
      >
        <div data-tauri-drag-region className="min-w-0">
          <p className="truncate text-sm font-medium">{view?.title || "Video"}</p>
          <p className="truncate text-chrome-micro text-chrome-muted">{summary}</p>
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

      {view?.notice ? (
        <p className="shrink-0 px-4 pb-2 text-chrome-micro break-words text-chrome-muted">
          {view.notice}
        </p>
      ) : null}

      {view?.downloading || (view?.done && view.total > 0) ? (
        <div className="shrink-0 px-4 pb-2">
          <Progress value={view.done ? 100 : percent} aria-label="Download progress" />
        </div>
      ) : null}

      <main className="flex min-h-0 flex-1 flex-col px-4 pb-2">
        {offerHelper && helperStatus ? (
          <HelperOffer
            open={offerHelper}
            status={helperStatus}
            onClose={() => setOfferDismissed(true)}
            onInstalled={(status) => void afterHelper(status)}
          />
        ) : null}

        {view?.cookiesOffered && !view.cookieConsent ? (
          <div className="mb-2 shrink-0 rounded-chrome bg-chrome-canvas px-3 py-2.5">
            <p className="text-chrome-micro text-chrome-menu-text">
              This site sent its cookies. B Download Manager can use them only
              for this site's own addresses, and it will not ask again.
            </p>
            <div className="mt-2 flex justify-end">
              <DialogAction
                tone="primary"
                disabled={consentBusy}
                onClick={() => void allowCookies()}
              >
                {consentBusy ? "Allowing…" : "Allow cookies"}
              </DialogAction>
            </div>
          </div>
        ) : null}

        {view?.state === "parsing" || !view ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-1 text-center">
            <p className="text-xs text-chrome-muted">
              {failure || "Reading the manifest…"}
            </p>
          </div>
        ) : view.state === "failed" ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-1 text-center">
            <p role="alert" className="text-xs break-words text-chrome-error">
              {view.error || failure || "This stream cannot be downloaded."}
            </p>
          </div>
        ) : (
          <>
            <FormatList
              rows={rows}
              value={selected}
              onSelect={setSelected}
              onActivate={(id) => void start(id)}
              disabled={busy || view.downloading || view.done}
              autoFocus
            />
            {actionError || (view.error && !view.downloading) ? (
              <p role="alert" className="pt-2 text-chrome-micro break-words text-chrome-error">
                {actionError || view.error}
              </p>
            ) : null}
          </>
        )}
      </main>

      <footer className="flex shrink-0 items-center gap-2 px-4 pb-4">
        <div className="min-w-0 flex-1">
          {view?.done && view.dest ? (
            <Tooltip content={view.dest}>
              <p className="truncate text-chrome-micro text-chrome-muted">
                {view.dest}
              </p>
            </Tooltip>
          ) : (
            <p className="truncate text-chrome-micro text-chrome-muted">
              {view?.downloading
                ? "The download keeps going if this window is closed."
                : view?.state === "ready" && !view.done
                  ? selected
                    ? "The file goes to your downloads folder."
                    : "Choose a quality."
                  : ""}
            </p>
          )}
        </div>

        {view?.done ? (
          <>
            <DialogAction
              tone="secondary"
              disabled={!view.dest}
              onClick={() => {
                if (view.dest) {
                  void openPath(view.dest).catch((error: unknown) =>
                    setActionError(describeError(error)),
                  );
                }
              }}
            >
              Show
            </DialogAction>
            <DialogAction tone="primary" onClick={() => void closeWindow()}>
              Close
            </DialogAction>
          </>
        ) : view?.state === "ready" ? (
          <>
            {view.downloading ? (
              <DialogAction tone="secondary" disabled={busy} onClick={() => void stop()}>
                Stop
              </DialogAction>
            ) : null}
            <DialogAction
              tone="quiet"
              onClick={() => void closeWindow()}
            >
              Close
            </DialogAction>
            {!view.downloading ? (
              <DialogAction
                tone="primary"
                icon={<DownloadGlyph className="size-3.5" />}
                disabled={!selected || busy}
                onClick={() => void start()}
              >
                {busy ? "Starting…" : "Download"}
              </DialogAction>
            ) : null}
          </>
        ) : (
          <DialogAction tone="primary" onClick={() => void closeWindow()}>
            Close
          </DialogAction>
        )}
      </footer>
    </div>
  );
}
