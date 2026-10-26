import { useRef, useState } from "react";

import { ChevronDown } from "@/components/icons/ui-icons";
import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import {
  STATUS_FILTER_LABELS,
  type DownloadStatus,
  type StatusFilter,
} from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

const STATUSES: DownloadStatus[] = [
  "downloading",
  "queued",
  "paused",
  "stopped",
  "completed",
  "error",
];

/**
 * The status filter of the list, on top of the categories pane.
 *
 * It narrows whatever the tree already shows, so "Video" and "Error" can be
 * asked for together. The button names the current choice instead of hiding it
 * behind a generic word.
 */
export function StatusFilterControl() {
  const { statusFilter, setStatusFilter } = useDownloads();
  const trigger = useRef<HTMLButtonElement>(null);
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);

  const rows: MenuRow<StatusFilter>[] = [
    { command: "all", label: STATUS_FILTER_LABELS.all, radio: true },
    ...STATUSES.map((status) => ({
      command: status,
      label: STATUS_FILTER_LABELS[status],
      radio: true,
    })),
  ];

  function toggle() {
    const box = trigger.current?.getBoundingClientRect();
    setAt(at || !box ? null : { x: box.left, y: box.bottom + 4 });
  }

  return (
    <div className="flex items-center gap-2">
      <span className="text-chrome-micro text-chrome-muted">Status</span>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="menu"
        aria-expanded={at !== null}
        onClick={toggle}
        // The menu closes on an outside pointerdown; without this the click on
        // the trigger would find it already closed and open it again.
        onPointerDown={(event) => event.stopPropagation()}
        className="inline-flex h-7 items-center gap-1.5 rounded-chrome bg-chrome-canvas px-2.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
      >
        {STATUS_FILTER_LABELS[statusFilter]}
        <ChevronDown className="size-3.5 text-chrome-muted" />
      </button>

      {at ? (
        <MenuPopup
          id="status-filter-menu"
          label="Filter by status"
          rows={rows}
          level={0}
          focus="first"
          at={at}
          checked={{ [statusFilter]: true }}
          onChoose={(row) => {
            setStatusFilter(row.command);
            setAt(null);
            trigger.current?.focus();
          }}
          onClose={(reason) => {
            setAt(null);
            if (reason !== "pointer") {
              trigger.current?.focus();
            }
          }}
        />
      ) : null}
    </div>
  );
}
