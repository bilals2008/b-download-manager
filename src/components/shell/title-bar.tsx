import { cn } from "@/lib/utils";
import { windowAction } from "@/lib/window";

import { AppMark } from "./app-mark";

const WINDOW_BUTTON =
  "flex h-8 w-12 items-center justify-center text-chrome-title-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

export function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      onDoubleClick={() => void windowAction("toggleMaximize")}
      className="flex h-8 items-stretch bg-chrome-title text-chrome-title-text"
    >
      <div
        data-tauri-drag-region
        className="pointer-events-none flex flex-1 items-center gap-2 px-2"
      >
        <AppMark />
        <span className="text-xs select-none">
          Internet Download Manager 6.43
        </span>
      </div>

      <div className="flex" onDoubleClick={(event) => event.stopPropagation()}>
        <button
          type="button"
          aria-label="Minimize window"
          title="Minimize"
          className={WINDOW_BUTTON}
          onClick={() => void windowAction("minimize")}
        >
          <svg
            className="size-3"
            viewBox="0 0 12 12"
            aria-hidden="true"
            focusable="false"
          >
            <path
              d="M2 6h8"
              stroke="currentColor"
              strokeWidth="1"
              strokeLinecap="round"
            />
          </svg>
        </button>
        <button
          type="button"
          aria-label="Maximize window"
          title="Maximize"
          className={WINDOW_BUTTON}
          onClick={() => void windowAction("toggleMaximize")}
        >
          <svg
            className="size-3"
            viewBox="0 0 12 12"
            aria-hidden="true"
            focusable="false"
          >
            <rect
              x="2.5"
              y="2.5"
              width="7"
              height="7"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        </button>
        <button
          type="button"
          aria-label="Close window"
          title="Close"
          className={cn(
            WINDOW_BUTTON,
            "hover:bg-chrome-danger hover:text-chrome-danger-text",
          )}
          onClick={() => void windowAction("close")}
        >
          <svg
            className="size-3"
            viewBox="0 0 12 12"
            aria-hidden="true"
            focusable="false"
          >
            <path
              d="m2.5 2.5 7 7m0-7-7 7"
              stroke="currentColor"
              strokeWidth="1"
              strokeLinecap="round"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
