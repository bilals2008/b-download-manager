import { useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openFile } from "@tauri-apps/plugin-dialog";

import { CheckGlyph } from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { Progress } from "@/components/ui/progress";
import { formatSize } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import {
  videoHelperForget,
  videoHelperImport,
  videoHelperInstall,
  type VideoHelperStatus,
} from "@/lib/media";

interface HelperEvent {
  name: string;
  downloaded: number;
  total: number | null;
}

/**
 * The one-time offer of the video helpers, shown as a modal dialog.
 *
 * `yt-dlp` reads the page; `ffmpeg` puts a separate picture and sound back
 * together. Both come from their official releases, are checked before they
 * run, and stay in the app's data folder. The video download itself always
 * stays this app's own.
 */
export function HelperOffer({
  open,
  status,
  onClose,
  onInstalled,
}: {
  open: boolean;
  status: VideoHelperStatus;
  onClose: () => void;
  onInstalled: (status: VideoHelperStatus) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<HelperEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [unchecked, setUnchecked] = useState(false);

  async function install() {
    setBusy(true);
    setError(null);
    setUnchecked(false);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<HelperEvent>("video-helper://progress", (event) => {
        setProgress(event.payload);
      });
      const ready = await videoHelperInstall();
      onInstalled(ready);
    } catch (installError) {
      setError(describeError(installError));
    } finally {
      unlisten?.();
      setBusy(false);
      setProgress(null);
    }
  }

  /** Adopt a `yt-dlp.exe` the person downloaded when the release host fails. */
  async function chooseFile() {
    const picked = await openFile({
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
        const ready = await videoHelperInstall();
        onInstalled(ready);
      } else {
        setUnchecked(true);
      }
    } catch (importError) {
      setError(describeError(importError));
    } finally {
      setBusy(false);
    }
  }

  /** Drop the person's chosen file and go back to the app's own download. */
  async function forget() {
    setBusy(true);
    setError(null);
    try {
      await videoHelperForget();
      setUnchecked(false);
    } catch (forgetError) {
      setError(describeError(forgetError));
    } finally {
      setBusy(false);
    }
  }

  const footer =
    busy ? null : unchecked ? (
      <>
        <DialogAction tone="quiet" onClick={forget}>
          Use app's own copy
        </DialogAction>
        <DialogAction
          tone="primary"
          onClick={() => onInstalled({ ...status, ytdlp: true })}
        >
          Continue
        </DialogAction>
      </>
    ) : (
      <>
        <DialogAction tone="quiet" onClick={onClose}>
          Not now
        </DialogAction>
        <DialogAction tone="primary" onClick={() => void install()}>
          {error ? "Try again" : "Yes, download them (recommended)"}
        </DialogAction>
      </>
    );

  return (
    <Dialog
      open={open}
      title="Get ready for video downloads"
      onClose={onClose}
      className="w-[25rem] max-w-[calc(100vw-2rem)]"
      footer={footer}
      dismissible={!busy}
    >
      <p className="text-chrome-micro leading-5 text-chrome-menu-text">
        Download videos from YouTube and 1,000+ sites
      </p>
      <p className="mt-1 text-chrome-micro leading-5 text-chrome-muted">
        Two free helpers make it work. Get them now, and saving a video is one
        click away when you find one.
      </p>

      <HelperRow
        installed={status.ytdlp}
        title="yt-dlp: finds the video"
        note="Reads a page and lists every quality the site offers, up to 4K, from YouTube, Vimeo and over a thousand other sites."
        glyph={<SearchGlyph className="size-4" />}
        progress={progress?.name === "yt-dlp" ? progress : null}
      />
      <HelperRow
        installed={status.ffmpeg}
        title="ffmpeg: joins picture and sound"
        note="Sites send their best quality as separate video and audio. ffmpeg puts them together into one file that plays anywhere."
        glyph={<MergeGlyph className="size-4" />}
        progress={progress?.name === "ffmpeg" ? progress : null}
      />
      <HelperRow
        installed={status.ytdlp && status.ffmpeg}
        title="Free, official and checked"
        note="Both are open source. B Download Manager gets them from their official releases and checks each file before it runs. About 100 MB, once."
        glyph={<CheckGlyph className="size-4" />}
      />

      {busy && !progress ? (
        <p aria-live="polite" className="mt-3 text-chrome-micro text-chrome-muted">
          Preparing the download…
        </p>
      ) : null}

      {unchecked ? (
        <p className="mt-3 text-chrome-micro text-chrome-muted">
          The file is saved. The publisher's list could not be reached, so it
          was not checked.
        </p>
      ) : null}

      {error ? (
        <div className="mt-3">
          <p role="alert" className="text-chrome-micro break-words text-chrome-error">
            {error}
          </p>
          <button
            type="button"
            onClick={() => void chooseFile()}
            className="mt-1 rounded-chrome text-chrome-micro text-chrome-muted underline underline-offset-2 transition-colors hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Use a downloaded yt-dlp file instead
          </button>
        </div>
      ) : null}

      {!busy ? (
        <p className="mt-3 text-chrome-micro leading-5 text-chrome-muted">
          Not now? The next video window offers them again.
        </p>
      ) : null}
    </Dialog>
  );
}

/** One benefit row of the offer: a glyph, a heading and one sentence. */
function HelperRow({
  installed,
  title,
  note,
  glyph,
  progress,
}: {
  installed: boolean;
  title: string;
  note: string;
  glyph: ReactNode;
  progress?: HelperEvent | null;
}) {
  const percent =
    progress && progress.total
      ? Math.min(100, (progress.downloaded / progress.total) * 100)
      : 0;

  return (
    <div className="mt-3 flex gap-3">
      <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-chrome bg-chrome-canvas text-chrome-focus">
        {glyph}
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-chrome-micro text-chrome-menu-text">
          {title}
          {installed ? (
            <span className="ml-2 text-chrome-muted">Ready</span>
          ) : null}
        </p>
        <p className="mt-0.5 text-chrome-micro leading-5 text-chrome-muted">{note}</p>
        {progress ? (
          <div className="mt-1.5">
            <Progress value={percent} aria-label={`${title} progress`} />
            <p aria-live="polite" className="mt-1 text-chrome-micro text-chrome-muted">
              {progress.total
                ? `${formatSize(progress.downloaded)} of ${formatSize(progress.total)}`
                : "Starting…"}
            </p>
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** The magnifier row's glyph: the helper finds the video on the page. */
function SearchGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      aria-hidden="true"
      className={className}
    >
      <circle cx="7" cy="7" r="4.2" />
      <path d="m10.2 10.2 3.3 3.3" />
    </svg>
  );
}

/** Two tracks becoming one, the one thing ffmpeg does here. */
function MergeGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
    >
      <path d="M2 4h3.5c1.2 0 2.3.6 3 1.6L10 8" />
      <path d="M2 12h3.5c1.2 0 2.3-.6 3-1.6L10 8" />
      <path d="M10 8h4" />
      <path d="m12 5.8 2.2 2.2L12 10.2" />
    </svg>
  );
}
