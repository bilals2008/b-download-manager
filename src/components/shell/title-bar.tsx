import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { windowAction } from "@/lib/window";

import { AppMark } from "./app-mark";

const WINDOW_BUTTON =
  "flex h-7 my-0.5 w-11 items-center justify-center rounded-chrome text-chrome-title-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

export function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className="flex h-8 items-stretch bg-chrome-title text-chrome-title-text"
    >
      <div className="pointer-events-none flex flex-1 items-center gap-2 px-2">
        <AppMark />
        <span className="text-xs select-none">B Download Manager</span>
      </div>

      <div
        className="flex items-center gap-0.5 pr-1.5"
        onDoubleClick={(event) => event.stopPropagation()}
      >
        <Tooltip content="Minimize" side="bottom">
          <button
            type="button"
            aria-label="Minimize window"
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
        </Tooltip>
        <Tooltip content="Maximize" side="bottom">
          <button
            type="button"
            aria-label="Maximize window"
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
        </Tooltip>
        <button
          type="button"
          aria-label="Close window"
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
