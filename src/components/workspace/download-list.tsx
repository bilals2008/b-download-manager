import { ChevronDown, ChevronUp } from "lucide-react";

import {
  formatDate,
  formatEta,
  formatSize,
  formatSpeed,
  progressOf,
  STATUS_LABELS,
  type Download,
  type SortKey,
} from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

interface Column {
  key: SortKey | null;
  label: string;
  className: string;
}

const COLUMNS: Column[] = [
  { key: "name", label: "File name", className: "" },
  { key: "size", label: "Size", className: "w-28 text-right" },
  { key: null, label: "Progress", className: "w-40" },
  { key: "speed", label: "Speed", className: "w-28 text-right" },
  { key: "eta", label: "Time left", className: "w-24 text-right" },
  { key: "status", label: "Status", className: "w-28" },
];

const STATUS_DOT: Record<Download["status"], string> = {
  downloading: "bg-icon-green-bright",
  queued: "bg-icon-amber",
  paused: "bg-icon-blue-bright",
  stopped: "bg-chrome-muted",
  completed: "bg-icon-green",
  error: "bg-icon-red-bright",
};

/**
 * The download list. Rows follow the filter of the categories pane, the sort
 * order of the "Arrange files" menu, and the selection of the workspace.
 */
export function DownloadList() {
  const { visible, selected, toggleSelect, sort, sortBy } = useDownloads();

  return (
    <div className="min-w-0 flex-1 overflow-auto bg-chrome-canvas">
      <table className="w-full border-collapse text-xs">
        <caption className="sr-only">Downloads</caption>
        <thead>
          <tr className="sticky top-0 z-10 bg-chrome-menu text-chrome-muted">
            {COLUMNS.map((column) => {
              const isSorted = column.key !== null && sort.key === column.key;
              return (
                <th
                  key={column.label}
                  scope="col"
                  className={`border-b border-chrome-line px-3 py-1.5 font-normal ${column.className}`}
                  aria-sort={
                    isSorted
                      ? sort.dir === "asc"
                        ? "ascending"
                        : "descending"
                      : column.key === null
                        ? undefined
                        : "none"
                  }
                >
                  {column.key === null ? (
                    column.label
                  ) : (
                    <button
                      type="button"
                      onClick={() => sortBy(column.key as SortKey)}
                      className={
                        "inline-flex items-center gap-1 hover:text-chrome-menu-text focus-visible:rounded-chrome focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
                        (column.className.includes("text-right")
                          ? "flex-row-reverse"
                          : "")
                      }
                    >
                      {column.label}
                      {isSorted ? (
                        sort.dir === "asc" ? (
                          <ChevronUp className="size-3.5" aria-hidden="true" />
                        ) : (
                          <ChevronDown className="size-3.5" aria-hidden="true" />
                        )
                      ) : null}
                    </button>
                  )}
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {visible.length === 0 ? (
            <tr>
              <td colSpan={COLUMNS.length} className="px-3 py-10 text-center">
                <p className="text-sm text-chrome-menu-text">No downloads here</p>
                <p className="mt-1 text-xs text-chrome-muted">
                  Pick another category in the categories pane, or add a download.
                </p>
              </td>
            </tr>
          ) : (
            visible.map((item) => {
              const isSelected = selected.includes(item.id);
              return (
                <tr
                  key={item.id}
                  aria-selected={isSelected}
                  onClick={(event) =>
                    toggleSelect(item.id, event.ctrlKey || event.metaKey)
                  }
                  className={
                    "border-b border-chrome-line/60 " +
                    (isSelected
                      ? "bg-chrome-menu-active"
                      : "hover:bg-chrome-menu-hover")
                  }
                >
                  <td className="px-3 py-1.5">
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        toggleSelect(item.id, event.ctrlKey || event.metaKey);
                      }}
                      title={item.url}
                      className="block max-w-0 text-left focus-visible:rounded-chrome focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
                    >
                      <span className="block truncate text-chrome-menu-text">
                        {item.fileName}
                      </span>
                      <span className="block truncate text-[11px] text-chrome-muted">
                        {item.savePath} · {formatDate(item.addedAt)}
                      </span>
                    </button>
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
                    {formatSize(item.size)}
                  </td>
                  <td className="px-3 py-1.5">
                    <div className="flex items-center gap-2">
                      <div
                        role="progressbar"
                        aria-valuenow={Math.round(progressOf(item))}
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-label={`${item.fileName} progress`}
                        className="h-1.5 flex-1 overflow-hidden rounded-full bg-chrome-line"
                      >
                        <div
                          className="h-full rounded-full bg-icon-green"
                          style={{ width: `${progressOf(item)}%` }}
                        />
                      </div>
                      <span className="w-10 shrink-0 text-right tabular-nums text-chrome-muted">
                        {Math.round(progressOf(item))}%
                      </span>
                    </div>
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
                    {formatSpeed(item.speed) || "—"}
                  </td>
                  <td className="px-3 py-1.5 text-right tabular-nums text-chrome-menu-text">
                    {item.status === "downloading" ? formatEta(item.eta) : "—"}
                  </td>
                  <td className="px-3 py-1.5">
                    <span className="inline-flex items-center gap-1.5 text-chrome-menu-text">
                      <span
                        className={`size-2 rounded-full ${STATUS_DOT[item.status]}`}
                        aria-hidden="true"
                      />
                      {STATUS_LABELS[item.status]}
                    </span>
                  </td>
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}
