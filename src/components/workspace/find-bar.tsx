import {
  ChevronDown,
  ChevronUp,
  CloseGlyph,
  SearchGlyph,
} from "@/components/icons/ui-icons";
import { useDownloads } from "@/state/download-store";

const ICON_BUTTON =
  "grid size-6 shrink-0 place-items-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent";

/**
 * The Find bar of the list, opened by Downloads > Find (Ctrl+F) or F3. It
 * searches the file name and the address, and Find Next steps through the
 * matches in the order the list shows, wrapping at the ends. It marks the
 * match by selecting its row, so the row highlights and the list scrolls to it.
 */
export function FindBar({ onClose }: { onClose: () => void }) {
  const { find, findMatches, findIndex, setFind, findNext } = useDownloads();

  const query = find.trim();
  const count =
    query.length === 0
      ? ""
      : findMatches.length === 0
        ? "No matches"
        : findIndex >= 0
          ? `${findIndex + 1} of ${findMatches.length}`
          : `${findMatches.length} found`;

  return (
    <div
      role="search"
      aria-label="Find downloads"
      className="flex h-9 shrink-0 items-center gap-1.5 border-b border-chrome-canvas bg-chrome-panel px-2"
    >
      <SearchGlyph className="size-3.5 shrink-0 text-chrome-muted" />

      <input
        autoFocus
        type="text"
        value={find}
        aria-label="Find downloads by name or address"
        placeholder="Find downloads"
        onChange={(event) => setFind(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            findNext(event.shiftKey ? -1 : 1);
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            findNext(1);
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            findNext(-1);
          } else if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
        className="h-6 min-w-0 flex-1 rounded-chrome border border-input bg-chrome-canvas px-2 text-xs text-chrome-menu-text transition-colors placeholder:text-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
      />

      <span
        aria-live="polite"
        className="min-w-16 shrink-0 text-right text-chrome-micro tabular-nums text-chrome-muted"
      >
        {count}
      </span>

      <button
        type="button"
        aria-label="Previous match"
        title="Previous match (Shift+Enter)"
        disabled={findMatches.length === 0}
        onClick={() => findNext(-1)}
        className={ICON_BUTTON}
      >
        <ChevronUp className="size-3.5" />
      </button>
      <button
        type="button"
        aria-label="Next match"
        title="Next match (Enter)"
        disabled={findMatches.length === 0}
        onClick={() => findNext(1)}
        className={ICON_BUTTON}
      >
        <ChevronDown className="size-3.5" />
      </button>
      <button
        type="button"
        aria-label="Close find bar"
        title="Close (Escape)"
        onClick={onClose}
        className={ICON_BUTTON}
      >
        <CloseGlyph className="size-3.5" />
      </button>
    </div>
  );
}
