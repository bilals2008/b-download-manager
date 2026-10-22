import { useEffect, useState } from "react";

import { CloseGlyph } from "@/components/icons/ui-icons";
import { formatSize, splitPath } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { openPath } from "@/lib/ipc";
import { closeWindow } from "@/lib/window";
import type { DownloadInfo } from "@/types";

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
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-popup text-chrome-menu-text">
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center justify-between gap-4 border-b border-chrome-line bg-linear-to-b from-chrome-toolbar to-chrome-popup py-1.5 pr-2 pl-4"
      >
        <span
          data-tauri-drag-region
          className="truncate text-sm text-chrome-title-text"
        >
          Download complete
        </span>
        <button
          type="button"
          onClick={() => void closeWindow()}
          aria-label="Close"
          className="flex size-6 shrink-0 items-center justify-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          <CloseGlyph className="size-3.5" />
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 px-4 py-3 text-xs">
        <p className="text-sm text-chrome-title-text">
          File {name} download complete.
        </p>
        <p className="text-chrome-muted">
          Downloaded {formatSize(bytes)} ({bytes} Bytes)
        </p>
        <p className="break-all text-chrome-muted">File saved as {info.dest}</p>
        {error ? (
          <p role="alert" className="break-words text-chrome-error">
            {error}
          </p>
        ) : null}
      </div>

      <div className="flex shrink-0 justify-end gap-2 border-t border-chrome-line px-4 py-2">
        <button
          type="button"
          onClick={() => void reveal(info.dest)}
          className="rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          Open file
        </button>
        <button
          type="button"
          onClick={() => void reveal(folder)}
          className="rounded-chrome bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          Open folder
        </button>
        <button
          type="button"
          autoFocus
          onClick={() => void closeWindow()}
          className="rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          Close
        </button>
      </div>
    </div>
  );
}
