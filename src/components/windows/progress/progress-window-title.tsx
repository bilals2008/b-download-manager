import {
  CloseGlyph,
  MinimizeToTrayGlyph,
} from "@/components/icons/ui-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { hideWindow, windowAction } from "@/lib/window";

const WINDOW_BUTTON =
  "flex h-8 w-9 items-center justify-center rounded-chrome text-chrome-title-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

interface ProgressWindowTitleProps {
  /** The file the download will become. */
  name: string;
  /** The destination it is being written to, shown on hover. */
  path: string;
}

/**
 * The title bar of the progress window.
 *
 * The window has no decorations, so this is its only frame: the file name on
 * the left, the percentage that also names the window, and the controls a tool
 * window needs. The tray button hides the window without stopping the
 * download; the tray menu lists it by name and brings it back. The bar is the
 * drag region.
 */
export function ProgressWindowTitle({
  name,
  path,
}: ProgressWindowTitleProps) {
  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center gap-3 border-b border-chrome-line pr-1 pl-3.5"
    >
      <Tooltip content={path} side="bottom" className="min-w-0 flex-1">
        <span
          data-tauri-drag-region
          className="block truncate text-xs font-medium text-chrome-title-text"
        >
          {name}
        </span>
      </Tooltip>

      <div
        className="flex items-center gap-0.5"
        onDoubleClick={(event) => event.stopPropagation()}
      >
        <Tooltip content="Minimize to tray" side="bottom">
          <button
            type="button"
            aria-label="Minimize to tray"
            className={WINDOW_BUTTON}
            onClick={() => void hideWindow()}
          >
            <MinimizeToTrayGlyph className="size-3.5" />
          </button>
        </Tooltip>
        <Tooltip content="Close" side="bottom">
          <button
            type="button"
            aria-label="Close window"
            className={`${WINDOW_BUTTON} hover:bg-chrome-danger hover:text-chrome-danger-text`}
            onClick={() => void windowAction("close")}
          >
            <CloseGlyph className="size-3" />
          </button>
        </Tooltip>
      </div>
    </header>
  );
}
