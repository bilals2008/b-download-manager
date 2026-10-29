import { useEffect, useState } from "react";

import { FileTypeIcon } from "@/components/icons/desktop-icons";
import { CheckGlyph, CloseGlyph } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import { categoryIcon, categoryOfFile, loadCategories } from "@/lib/categories";
import { formatSize, splitPath } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { openPath } from "@/lib/ipc";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";
import type { DownloadInfo } from "@/types";

/**
 * The window that opens when a file finishes.
 *
 * The wording follows the original product's messages: a completion line
 * naming the file, the size taken with the raw byte count in parentheses,
 * and the path the file was saved to, plus an "Open folder" button
 * (`ANALYSIS.md`, section 9.4). The download is handed in by the backend
 * through `window.__BDM_COMPLETE__` before this renders.
 */
export function DownloadCompleteWindow({ info }: { info: DownloadInfo }) {
  const [error, setError] = useState<string | null>(null);
  const [categories] = useState(loadCategories);
  const { dir, base } = splitPath(info.dest);
  const name = base || "The file";
  const folder = dir || ".";
  const bytes = info.total || info.downloaded;
  const category = categoryOfFile(name, categories);

  useEffect(() => {
    void showWindowWhenPainted();
  }, []);

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
    // The rounded edge is the frame, the same way the progress window draws
    // its own: no border, one corner shape.
    <div className="flex h-dvh flex-col overflow-hidden rounded-lg bg-chrome-popup text-chrome-menu-text">
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
            <FileTypeIcon
              category={category}
              icon={categoryIcon(categories, category)}
              name={name}
              className="size-9"
            />
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
          <Tooltip
            content={info.dest}
            side="top"
            className="mt-0.5 block"
          >
            <p className="line-clamp-2 font-mono text-chrome-micro break-all text-chrome-menu-text">
              {info.dest}
            </p>
          </Tooltip>
        </div>

        {error ? (
          <p role="alert" className="break-words text-chrome-micro text-chrome-error">
            {error}
          </p>
        ) : null}
      </div>

      <div className="flex shrink-0 justify-end gap-2 px-4 py-3">
        <DialogAction onClick={() => void reveal(folder)}>
          Open folder
        </DialogAction>
        <DialogAction onClick={() => void reveal(info.dest)}>
          Open file
        </DialogAction>
        <DialogAction tone="primary" autoFocus onClick={() => void closeWindow()}>
          Close
        </DialogAction>
      </div>
    </div>
  );
}
