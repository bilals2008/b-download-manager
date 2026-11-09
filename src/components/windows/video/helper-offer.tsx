import { useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import { DialogAction } from "@/components/ui/dialog-action";
import { Progress } from "@/components/ui/progress";
import { formatSize } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { videoHelperImport, videoHelperInstall } from "@/lib/media";

interface HelperEvent {
  downloaded: number;
  total: number | null;
}

/**
 * The one-time offer of the video helper, inside the panel of a YouTube video.
 *
 * The helper only reads the page; the download stays this app's own. It is
 * fetched from its official release, checked against the publisher's checksum
 * list, and kept in the app's data folder.
 */
export function HelperOffer({
  onInstalled,
  onDismiss,
}: {
  onInstalled: () => void;
  onDismiss: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<HelperEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [unchecked, setUnchecked] = useState(false);

  async function install() {
    setBusy(true);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<HelperEvent>("video-helper://progress", (event) => {
        setProgress(event.payload);
      });
      await videoHelperInstall();
      onInstalled();
    } catch (installError) {
      setError(describeError(installError));
    } finally {
      unlisten?.();
      setBusy(false);
    }
  }

  /** Adopt a `yt-dlp.exe` the person downloaded when the release host fails. */
  async function chooseFile() {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "yt-dlp", extensions: ["exe"] }],
    });
    if (typeof picked !== "string") {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (await videoHelperImport(picked)) {
        onInstalled();
      } else {
        setUnchecked(true);
      }
    } catch (importError) {
      setError(describeError(importError));
    } finally {
      setBusy(false);
    }
  }

  const percent =
    progress && progress.total
      ? Math.min(100, (progress.downloaded / progress.total) * 100)
      : 0;

  return (
    <section className="mb-2 shrink-0 rounded-chrome bg-chrome-canvas px-3 py-2.5">
      <p className="text-chrome-micro text-chrome-menu-text">
        One free helper for YouTube
      </p>
      <p className="mt-1 text-chrome-micro text-chrome-muted">
        YouTube changes often. yt-dlp reads the page and finds the video, so the
        download starts. About 25 MB, once; it is checked before it runs.
      </p>

      {busy ? (
        <div className="mt-2">
          <Progress value={percent} aria-label="Helper download progress" />
          <p aria-live="polite" className="mt-1 text-chrome-micro text-chrome-muted">
            {progress && progress.total
              ? `${formatSize(progress.downloaded)} of ${formatSize(progress.total)}`
              : "Downloading the helper…"}
          </p>
        </div>
      ) : unchecked ? (
        <div className="mt-2">
          <p className="text-chrome-micro text-chrome-muted">
            The file is saved. The publisher's list could not be reached, so it
            was not checked.
          </p>
          <div className="mt-2 flex justify-end">
            <DialogAction tone="primary" onClick={onInstalled}>
              Continue
            </DialogAction>
          </div>
        </div>
      ) : (
        <div className="mt-2 flex justify-end gap-2">
          <DialogAction tone="quiet" onClick={onDismiss}>
            Not now
          </DialogAction>
          <DialogAction tone="quiet" onClick={() => void chooseFile()}>
            Use a file…
          </DialogAction>
          <DialogAction tone="primary" onClick={() => void install()}>
            Install helper
          </DialogAction>
        </div>
      )}

      {error ? (
        <p role="alert" className="mt-2 text-chrome-micro break-words text-chrome-error">
          {error}
        </p>
      ) : null}
    </section>
  );
}
