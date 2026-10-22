import { useRef } from "react";

import { FileTypeIcon, StatusMark } from "@/components/icons/desktop-icons";
import { SelectionCheckbox } from "@/components/workspace/selection-checkbox";
import { Progress } from "@/components/ui/progress";
import {
  formatEta,
  formatSize,
  formatSpeed,
  progressOf,
  STATUS_LABELS,
  type Download,
} from "@/lib/downloads";
import { cn } from "@/lib/utils";
import type { SelectMode } from "@/state/download-store";

interface DownloadRowProps {
  item: Download;
  selected: boolean;
  /** How the click picks: plain, Ctrl or Cmd, Shift for a range, both to add one. */
  onSelect: (mode: SelectMode) => void;
  /** Open the menu of the row, the way a right click does. */
  onOpenMenu: (at: { x: number; y: number }, trigger: HTMLElement | null) => void;
}

/**
 * One download, the way IDM shows it: the name, the size, the progress as a
 * percentage, the speed, the time left and the status. The folder is not
 * repeated under the name; it is there when the row is opened, in its menu
 * and in the categories pane. Nothing opens under the row, so the list stays
 * a list.
 */
export function DownloadRow({
  item,
  selected,
  onSelect,
  onOpenMenu,
}: DownloadRowProps) {
  const nameRef = useRef<HTMLButtonElement>(null);
  const status = STATUS_LABELS[item.status];
  const percent = progressOf(item);
  const knownSize = item.size > 0;

  /** The menu of a row opens at the pointer, or at the row for the keyboard. */
  function openMenu(at?: { x: number; y: number }) {
    const box = nameRef.current?.getBoundingClientRect();
    onOpenMenu(
      at ?? { x: box?.left ?? 0, y: box?.bottom ?? 0 },
      nameRef.current,
    );
  }

  return (
    <tr
      data-download-id={item.id}
      aria-selected={selected}
      onClick={(event) => onSelect(modeOf(event))}
      onContextMenu={(event) => {
        event.preventDefault();
        openMenu({ x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event) => {
        if (
          event.key === "ContextMenu" ||
          (event.shiftKey && event.key === "F10")
        ) {
          event.preventDefault();
          openMenu();
        }
      }}
      className={cn(
        "cursor-default select-none border-b border-chrome-line/60 transition-colors",
        selected
          ? "bg-chrome-menu-active"
          : "hover:bg-chrome-menu-hover",
      )}
    >
      <td className="border-b border-chrome-line/60 px-3 py-1.5">
        <div className="flex min-w-0 items-center gap-2.5">
          <SelectionCheckbox
            checked={selected}
            label={`Select ${item.fileName}`}
            onToggle={() => onSelect("toggle")}
          />

          <FileTypeIcon
            category={item.category}
            className="size-5 shrink-0"
          />

          <button
            ref={nameRef}
            type="button"
            title={item.url}
            onClick={(event) => {
              event.stopPropagation();
              onSelect(modeOf(event));
            }}
            className="min-w-0 flex-1 text-left focus-visible:rounded-chrome focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <span className="block truncate text-chrome-menu-text">
              {item.fileName}
            </span>
            {item.error ? (
              <span className="mt-0.5 block truncate text-chrome-micro text-icon-red-bright">
                {item.error}
              </span>
            ) : null}
          </button>
        </div>
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
        {knownSize ? formatSize(item.size) : "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
        {knownSize ? (
          <>
            <span>{percent.toFixed(2)}%</span>
            <Progress
              value={Math.min(100, percent)}
              aria-label={`${item.fileName} progress`}
              className="mt-1 h-1 bg-chrome-popup"
            />
          </>
        ) : (
          "-"
        )}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
        {formatSpeed(item.speed) || "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
        {item.status === "downloading" ? formatEta(item.eta) : "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-1.5">
        <span
          className="inline-flex items-center gap-1.5 text-chrome-menu-text"
          title={item.error ?? status}
        >
          <StatusMark status={item.status} className="size-4 shrink-0" />
          {status}
        </span>
      </td>
    </tr>
  );
}

/** The selection mode the pointer and its modifier keys ask for. */
function modeOf(event: {
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}): SelectMode {
  if (event.shiftKey) {
    return event.ctrlKey || event.metaKey ? "add-range" : "range";
  }
  return event.ctrlKey || event.metaKey ? "toggle" : "replace";
}
