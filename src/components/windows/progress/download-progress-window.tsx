import { useEffect, useState } from "react";

import {
  DownloadStatusTab,
  type ProgressAction,
} from "@/components/windows/progress/download-status-tab";
import {
  ProgressTabs,
  type ProgressTabId,
} from "@/components/windows/progress/progress-tabs";
import { ProgressWindowTitle } from "@/components/windows/progress/progress-window-title";
import { SpeedLimitTab } from "@/components/windows/progress/speed-limit-tab";
import { fileNameFromUrl, splitPath } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import {
  cancelDownload,
  openPath,
  pauseDownload,
  resumeDownload,
  setProgressDetails,
} from "@/lib/ipc";
import { progressPercentText } from "@/lib/progress";
import { closeWindow } from "@/lib/window";
import { useAppSpeedLimit } from "@/state/use-app-speed-limit";
import { useLiveDownload } from "@/state/use-live-download";
import type { DownloadInfo, DownloadStatus } from "@/types";

/**
 * The "Download progress" window of one download.
 *
 * The state comes from the engine: the copy the window was opened with, then
 * every `download://update`. The controls call the same Tauri commands the
 * list calls, so the window and the row cannot disagree. Closing the window
 * leaves the download running; only Cancel stops it.
 *
 * The download is handed in by the backend through `window.__IDM_PROGRESS__`
 * before this renders.
 */
export function DownloadProgressWindow({ initial }: { initial: DownloadInfo }) {
  const { info, speed, eta } = useLiveDownload(initial);
  const [tab, setTab] = useState<ProgressTabId>("status");
  const [detailsOpen, setDetailsOpen] = useState(true);
  const [busy, setBusy] = useState<ProgressAction | null>(null);
  // A refusal is coupled to the state it was refused in. When the engine moves
  // on — the download finishes while Pause was being sent, for instance — the
  // message that described the old state goes with it.
  const [actionError, setActionError] = useState<{
    message: string;
    status: DownloadStatus;
  } | null>(null);
  const visibleActionError =
    actionError !== null && actionError.status === info.status
      ? actionError.message
      : null;

  // The cap of the whole application is a fact this window only reports; it is
  // owned by Options → Connection, and the hook follows it there.
  const appLimit = useAppSpeedLimit();

  const { base } = splitPath(info.dest);
  const name = base || fileNameFromUrl(info.url);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      // A control that already used Escape — the unit list of the speed
      // limiter closes itself with it — keeps the window open.
      if (event.key === "Escape" && !event.defaultPrevented) {
        void closeWindow();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function act(action: ProgressAction) {
    setBusy(action);
    setActionError(null);
    try {
      if (action === "pause") {
        await pauseDownload(info.id);
      } else if (action === "resume") {
        await resumeDownload(info.id);
      } else {
        await cancelDownload(info.id);
        // The window has nothing left to show: the row in the list keeps the
        // bytes that were received, and can be resumed from there.
        await closeWindow();
      }
    } catch (failure) {
      setActionError({ message: describeError(failure), status: info.status });
    } finally {
      setBusy(null);
    }
  }

  async function openFile() {
    setActionError(null);
    try {
      await openPath(info.dest);
    } catch (failure) {
      setActionError({ message: describeError(failure), status: info.status });
    }
  }

  async function toggleDetails() {
    const open = !detailsOpen;
    setDetailsOpen(open);
    setActionError(null);
    try {
      // The window is sized by its chrome, not by its content, so the page
      // asks the shell for the height that matches the section.
      await setProgressDetails(open);
    } catch (failure) {
      // The window did not change size, so an opened section would be clipped:
      // put the page back the way the window still is.
      setDetailsOpen(!open);
      setActionError({ message: describeError(failure), status: info.status });
    }
  }

  return (
    // The corners belong to the window, not to this box: `window_style.rs`
    // asks the compositor to round the whole frame. A radius here as well cut
    // the border into a rounded inner corner inside a square window on older
    // Windows, and into a different curve inside the rounded one on Windows 11.
    <div className="flex h-dvh flex-col overflow-hidden border border-chrome-line bg-chrome-popup text-chrome-menu-text">
      <ProgressWindowTitle
        name={name}
        path={info.dest}
        percent={progressPercentText(info)}
      />

      <div className="flex min-h-0 flex-1 flex-col px-4 pt-1 pb-3">
        <ProgressTabs active={tab} onChange={setTab} />

        <div className="flex min-h-0 flex-1 flex-col pt-3">
          {tab === "status" ? (
            <DownloadStatusTab
              info={info}
              speed={speed}
              eta={eta}
              detailsOpen={detailsOpen}
              onToggleDetails={() => void toggleDetails()}
              onPause={() => void act("pause")}
              onResume={() => void act("resume")}
              onCancel={() => void act("cancel")}
              onOpen={() => void openFile()}
              onClose={() => void closeWindow()}
              busy={busy}
              actionError={visibleActionError}
            />
          ) : (
            <SpeedLimitTab info={info} appLimit={appLimit} />
          )}
        </div>
      </div>
    </div>
  );
}
