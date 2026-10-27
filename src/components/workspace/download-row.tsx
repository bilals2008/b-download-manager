import { useRef } from "react";

import { FileTypeIcon, StatusMark } from "@/components/icons/desktop-icons";
import { Tooltip } from "@/components/ui/tooltip";
import type { CategoryIcon } from "@/lib/category-icons";
import { SelectionCheckbox } from "@/components/workspace/selection-checkbox";
import { DownloadProgress } from "@/components/workspace/download-progress";
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
  /** The saved icon of the row's category, so a custom icon reaches the row. */
  icon?: CategoryIcon;
  selected: boolean;
  /** The checkbox picks this row alone. */
  onToggle: () => void;
  /** A click on the row: replace the selection, or extend it with Shift/Ctrl. */
  onSelect: (mode: SelectMode) => void;
  /** Open the menu of the row, the way a right click does. */
  onOpenMenu: (at: { x: number; y: number }, trigger: HTMLElement | null) => void;
  /** Open the progress window of the row, the way a double click does. */
  onOpen: () => void;
  /** Open the window that explains a failed row and offers to recover it. */
  onOpenFailure: () => void;
}

/**
 * One download as a row: the name, the size, the progress as a percentage,
 * the speed, the time left and the status. The folder is not repeated under
 * the name; it is there when the row is opened, in its menu and in the
 * categories pane. Nothing opens under the row, so the list stays
 * a list.
 */
export function DownloadRow({
  item,
  icon,
  selected,
  onToggle,
  onSelect,
  onOpenMenu,
  onOpen,
  onOpenFailure,
}: DownloadRowProps) {
  const nameRef = useRef<HTMLDivElement>(null);
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
      onClick={(event) => {
        if (event.shiftKey) {
          onSelect("range");
        } else if (event.ctrlKey || event.metaKey) {
          onSelect("toggle");
        } else {
          onSelect("replace");
        }
      }}
      onDoubleClick={(event) => {
        // The checkbox already answered the double click with two toggles.
        if ((event.target as HTMLElement).closest("input")) {
          return;
        }
        onOpen();
      }}
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
        // The scroll margin keeps a row clear of the sticky header when the
        // list brings it into view for the keyboard or for Find.
        "scroll-mt-9 select-none transition-colors",
        selected
          ? "bg-chrome-menu-active"
          : "hover:bg-chrome-menu-hover",
      )}
    >
      <td className="px-3 py-2">
        <div className="flex min-w-0 items-center gap-2.5">
          <SelectionCheckbox
            checked={selected}
            label={`Select ${item.fileName}`}
            onToggle={onToggle}
          />

          <FileTypeIcon
            category={item.category}
            icon={icon}
            className="size-5 shrink-0"
          />

          <div ref={nameRef} className="min-w-0 flex-1">
            <Tooltip
              content={item.url}
              side="bottom"
              className="block min-w-0"
            >
              <span className="block truncate text-chrome-menu-text">
                {item.fileName}
              </span>
            </Tooltip>
            {item.error ? (
              <Tooltip
                content={`${item.error} — open the failure details`}
                side="bottom"
                className="block min-w-0"
              >
                <button
                  type="button"
                  onClick={(event) => {
                    // The row's own click would only move the selection; this
                    // opens the window that offers the recovery actions.
                    event.stopPropagation();
                    onOpenFailure();
                  }}
                  onDoubleClick={(event) => event.stopPropagation()}
                  className="mt-0.5 block w-full truncate rounded-chrome text-left text-chrome-micro text-icon-red-bright transition-colors hover:underline focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
                >
                  {item.error}
                </button>
              </Tooltip>
            ) : null}
          </div>
        </div>
      </td>

      <td className="px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {knownSize ? formatSize(item.size) : "-"}
      </td>

      <td className="px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {knownSize ? (
          <DownloadProgress
            value={percent}
            status={item.status}
            label={`${item.fileName} progress`}
          />
        ) : (
          "-"
        )}
      </td>

      <td className="px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {formatSpeed(item.speed) || "-"}
      </td>

      <td className="px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {item.status === "downloading" ? formatEta(item.eta) : "-"}
      </td>

      <td className="px-3 py-2">
        <span className="inline-flex items-center gap-1.5 text-chrome-menu-text">
          <StatusMark status={item.status} className="size-4 shrink-0" />
          {status}
        </span>
      </td>
    </tr>
  );
}
