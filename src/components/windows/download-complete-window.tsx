import { useEffect, useState } from "react";

import { FileTypeIcon } from "@/components/icons/desktop-icons";
import { CheckGlyph, CloseGlyph } from "@/components/icons/ui-icons";
import { categoryOfFile, formatSize, splitPath } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { openPath } from "@/lib/ipc";
import { closeWindow } from "@/lib/window";
import type { DownloadInfo } from "@/types";

const BUTTON =
  "rounded-chrome px-3 py-1.5 text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

/**
 * The window IDM opens when a file finishes.
 *
 * The words are the ones in `IDMan.exe`: "File %s download complete.",
 * "Downloaded %s (%I64d Bytes)" and "File saved as %s", with its "Open folder"
 * button (`ANALYSIS.md`, section 9.4). The download is handed in by the
 * backend through `window.__IDM_COMPLETE__` before this renders.
 */
export function DownloadCompleteWindow({ info }: { info: DownloadInfo }) {
  const [error, setError] = useState<string | null>(null);
  const { dir, base } = splitPath(info.dest);
  const name = base || "The file";
  const folder = dir || ".";
  const bytes = info.total || info.downloaded;

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        void closeWindow();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function reveal(target: string) {
    try {
      await openPath(target);
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  return (
    <div className="flex h-dvh flex-col overflow-hidden rounded-lg border border-chrome-line bg-chrome-popup text-chrome-menu-text">
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center justify-between gap-4 py-2 pr-2 pl-4"
      >
        <span
          data-tauri-drag-region
          className="truncate text-chrome-micro font-medium tracking-wide text-chrome-muted uppercase"
        >
          Download complete
        </span>
        <button
          type="button"
          onClick={() => void closeWindow()}
          aria-label="Close"
          className="flex size-7 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          <CloseGlyph className="size-3.5" />
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-3 px-4 pb-1 text-xs">
        <div className="flex items-start gap-3">
          <span className="relative shrink-0">
            <FileTypeIcon category={categoryOfFile(name)} className="size-9" />
            <span className="absolute -right-1 -bottom-1 grid size-4 place-items-center rounded-full bg-success text-success-foreground ring-2 ring-chrome-popup">
              <CheckGlyph className="size-2.5" />
            </span>
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-sm break-words text-chrome-title-text">
              File {name} download complete.
            </p>
            <p className="mt-0.5 text-chrome-micro tabular-nums text-chrome-muted">
              Downloaded {formatSize(bytes)} ({bytes} Bytes)
            </p>
          </div>
        </div>

        <div className="min-h-0 flex-1 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 py-2">
          <p className="text-chrome-micro text-chrome-muted">File saved as</p>
          <p
            title={info.dest}
            className="mt-0.5 line-clamp-2 font-mono text-chrome-micro break-all text-chrome-menu-text"
          >
            {info.dest}
          </p>
        </div>

        {error ? (
          <p role="alert" className="break-words text-chrome-micro text-chrome-error">
            {error}
          </p>
        ) : null}
      </div>

      <div className="flex shrink-0 justify-end gap-2 px-4 py-3">
        <button
          type="button"
          onClick={() => void reveal(folder)}
          className={`${BUTTON} bg-chrome-panel text-chrome-menu-text hover:bg-chrome-menu-hover`}
        >
          Open folder
        </button>
        <button
          type="button"
          onClick={() => void reveal(info.dest)}
          className={`${BUTTON} bg-chrome-panel text-chrome-menu-text hover:bg-chrome-menu-hover`}
        >
          Open file
        </button>
        <button
          type="button"
          autoFocus
          onClick={() => void closeWindow()}
          className={`${BUTTON} bg-primary font-medium text-primary-foreground hover:opacity-90`}
        >
          Close
        </button>
      </div>
    </div>
  );
}
