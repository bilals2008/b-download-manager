import {
  PauseGlyph,
  PlayGlyph,
  StopGlyph,
  TrashGlyph,
} from "@/components/icons/action-icons";
import { useDownloads } from "@/state/download-store";

/** Desk density for the bar's buttons: hairline, 24px tall, plain but Delete. */
const BUTTON =
  "inline-flex h-6 items-center gap-1 rounded-chrome border border-chrome-line bg-chrome-panel px-2 text-chrome-micro whitespace-nowrap hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45";

/**
 * The bar above the list while rows are picked. It names the count and puts
 * every batch command in one place, so a multi-selection is a visible mode
 * rather than a hidden Ctrl+click state.
 */
export function SelectionBar() {
  const {
    downloads,
    visible,
    selected,
    selectAll,
    clearSelection,
    applyToSelection,
  } = useDownloads();

  if (selected.length === 0) {
    return null;
  }

  const items = downloads.filter((item) => selected.includes(item.id));
  const canResume = items.some((item) => item.status !== "completed");
  const canPause = items.some(
    (item) => item.status === "downloading" || item.status === "queued",
  );
  const canStop = items.some(
    (item) =>
      item.status === "downloading" ||
      item.status === "queued" ||
      item.status === "paused",
  );
  const allPicked = visible.length > 0 && items.length >= visible.length;

  return (
    <div
      aria-label="Selected downloads"
      role="group"
      className="flex h-8 shrink-0 items-center gap-1.5 overflow-x-auto border-b border-chrome-line bg-chrome-panel px-2 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
    >
      <span
        aria-live="polite"
        className="mr-1 flex shrink-0 items-baseline gap-1 text-xs text-chrome-menu-text"
      >
        <span className="font-semibold tabular-nums">{items.length}</span>
        <span className="text-chrome-muted">selected</span>
      </span>

      <button
        type="button"
        disabled={!canResume}
        onClick={() => applyToSelection("resume")}
        className={BUTTON}
      >
        <PlayGlyph className="size-3.5" />
        Resume
      </button>
      <button
        type="button"
        disabled={!canPause}
        onClick={() => applyToSelection("pause")}
        className={BUTTON}
      >
        <PauseGlyph className="size-3.5" />
        Pause
      </button>
      <button
        type="button"
        disabled={!canStop}
        onClick={() => applyToSelection("stop")}
        className={BUTTON}
      >
        <StopGlyph className="size-3.5" />
        Stop
      </button>
      <button
        type="button"
        onClick={() => applyToSelection("delete")}
        className={`${BUTTON} text-chrome-error hover:border-chrome-danger hover:bg-chrome-danger hover:text-chrome-danger-text`}
      >
        <TrashGlyph className="size-3.5" />
        Delete
      </button>

      <span className="ml-auto flex shrink-0 items-center gap-1.5">
        {allPicked ? null : (
          <button
            type="button"
            onClick={selectAll}
            title="Select every download in this view"
            className={BUTTON}
          >
            Select all
          </button>
        )}
        <button
          type="button"
          onClick={clearSelection}
          title="Clear the selection (Esc)"
          className={BUTTON}
        >
          Clear
        </button>
      </span>
    </div>
  );
}
