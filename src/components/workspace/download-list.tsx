import { useRef, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  ChevronsUpDown,
  Inbox,
  LoaderCircle,
  Plus,
  X,
} from "lucide-react";

import { DOWNLOAD_COLUMNS } from "@/components/workspace/download-columns";
import { DownloadContextMenu } from "@/components/workspace/download-context-menu";
import { DownloadRow } from "@/components/workspace/download-row";
import { useColumnWidths } from "@/components/workspace/use-column-widths";
import type { Download, SortKey } from "@/lib/downloads";
import { cn } from "@/lib/utils";
import { useDownloads } from "@/state/download-store";

interface DownloadListProps {
  onAddDownload: () => void;
}

/**
 * The download list. Rows follow the filter of the categories pane, the sort
 * order of the "Arrange files" menu, and the selection of the workspace.
 * Columns can be dragged wider and a row opens into its engine details.
 */
export function DownloadList({ onAddDownload }: DownloadListProps) {
  const {
    downloads,
    visible,
    selected,
    toggleSelect,
    sort,
    sortBy,
    filter,
    setFilter,
    loading,
    error,
    dismissError,
    reload,
  } = useDownloads();
  const { widths, tableRef, beginResize, moveResize, endResize, resizeBy, resetWidths } =
    useColumnWidths(DOWNLOAD_COLUMNS);
  const [expanded, setExpanded] = useState<number[]>([]);
  // The row whose menu is open, and where the pointer was.
  const [menu, setMenu] = useState<{
    item: Download;
    at: { x: number; y: number };
  } | null>(null);
  // The control that opened the menu, so the focus goes back to it.
  const menuTrigger = useRef<HTMLElement | null>(null);

  function openMenu(
    item: Download,
    at: { x: number; y: number },
    trigger: HTMLElement | null,
  ) {
    menuTrigger.current = trigger;
    // A right click acts on the row under the pointer, so that row joins the
    // selection when it was not in it.
    if (!selected.includes(item.id)) {
      toggleSelect(item.id, false);
    }
    setMenu({ item, at });
  }

  function closeMenu() {
    setMenu(null);
    menuTrigger.current?.focus();
  }

  function toggleExpanded(id: number) {
    setExpanded((current) =>
      current.includes(id)
        ? current.filter((value) => value !== id)
        : [...current, id],
    );
  }

  const filterLabel =
    filter === "all"
      ? "All Downloads"
      : filter === "unfinished"
        ? "Unfinished"
        : filter === "finished"
          ? "Finished"
          : filter;

  return (
    <section
      aria-label="Downloads"
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-chrome-canvas"
    >
      {error ? (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-chrome-line bg-chrome-panel px-3 py-2 text-xs text-chrome-menu-text"
        >
          <AlertTriangle
            className="mt-0.5 size-4 shrink-0 text-icon-amber"
            aria-hidden="true"
          />
          <p className="min-w-0 flex-1 break-words">{error}</p>
          <button
            type="button"
            onClick={reload}
            className="shrink-0 rounded-chrome border border-chrome-line px-2 py-0.5 text-xs hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Retry
          </button>
          <button
            type="button"
            aria-label="Dismiss error"
            onClick={dismissError}
            className="grid size-5 shrink-0 place-items-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <X className="size-3.5" aria-hidden="true" />
          </button>
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
        <table
          ref={tableRef}
          className="w-full table-fixed border-separate border-spacing-0 text-xs"
        >
          <caption className="sr-only">
            Downloads, with file name, size, progress, speed, time left and
            status
          </caption>
          <colgroup>
            {DOWNLOAD_COLUMNS.map((column) => (
              <col
                key={column.id}
                style={{ width: `${widths[column.id] ?? column.width}%` }}
              />
            ))}
          </colgroup>

          <thead>
            <tr>
              {DOWNLOAD_COLUMNS.map((column, index) => {
                const isSorted = column.sortKey !== null && sort.key === column.sortKey;
                const SortIcon = !isSorted
                  ? ChevronsUpDown
                  : sort.dir === "asc"
                    ? ChevronUp
                    : ChevronDown;

                return (
                  <th
                    key={column.id}
                    scope="col"
                    aria-sort={
                      isSorted
                        ? sort.dir === "asc"
                          ? "ascending"
                          : "descending"
                        : column.sortKey
                          ? "none"
                          : undefined
                    }
                    className={cn(
                      "sticky top-0 z-10 border-b border-chrome-line bg-chrome-menu px-3 py-2 text-chrome-micro font-medium tracking-wide text-chrome-muted",
                      column.align === "right" ? "text-right" : "text-left",
                    )}
                  >
                    {column.sortKey ? (
                      <button
                        type="button"
                        title={`Sort by ${column.label}`}
                        onClick={() => sortBy(column.sortKey as SortKey)}
                        className={cn(
                          "inline-flex items-center gap-1 rounded-chrome hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
                          column.align === "right" && "flex-row-reverse",
                        )}
                      >
                        <span>{column.label}</span>
                        <SortIcon
                          className={cn(
                            "size-3.5",
                            !isSorted && "opacity-40",
                          )}
                          aria-hidden="true"
                        />
                      </button>
                    ) : (
                      column.label
                    )}

                    <div
                      role="separator"
                      aria-orientation="vertical"
                      aria-label={`Resize ${column.label} column`}
                      aria-valuenow={Math.round(widths[column.id] ?? column.width)}
                      aria-valuemin={column.minWidth}
                      aria-valuemax={100}
                      tabIndex={0}
                      title="Drag to resize. Arrow keys resize. Double-click resets all columns."
                      onPointerDown={(event) => {
                        event.preventDefault();
                        event.currentTarget.setPointerCapture(event.pointerId);
                        beginResize(index, event.clientX);
                      }}
                      onPointerMove={(event) => {
                        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
                          moveResize(event.clientX);
                        }
                      }}
                      onPointerUp={(event) => {
                        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
                          event.currentTarget.releasePointerCapture(event.pointerId);
                        }
                        endResize();
                      }}
                      onPointerCancel={endResize}
                      onLostPointerCapture={endResize}
                      onDoubleClick={resetWidths}
                      onKeyDown={(event) => {
                        if (event.key === "ArrowLeft") {
                          event.preventDefault();
                          resizeBy(index, event.shiftKey ? -5 : -1);
                        } else if (event.key === "ArrowRight") {
                          event.preventDefault();
                          resizeBy(index, event.shiftKey ? 5 : 1);
                        } else if (event.key === "Home") {
                          event.preventDefault();
                          resetWidths();
                        }
                      }}
                      className="absolute inset-y-0 right-0 z-20 w-2 cursor-col-resize touch-none focus-visible:outline-none after:absolute after:inset-y-1 after:right-0 after:w-px after:bg-chrome-line hover:after:bg-chrome-focus focus-visible:after:bg-chrome-focus"
                    />
                  </th>
                );
              })}
            </tr>
          </thead>

          <tbody>
            {loading && downloads.length === 0 ? (
              <tr>
                <td colSpan={DOWNLOAD_COLUMNS.length} className="px-3 py-16">
                  <p
                    role="status"
                    className="flex items-center justify-center gap-2 text-xs text-chrome-muted"
                  >
                    <LoaderCircle
                      className="size-4 motion-safe:animate-spin"
                      aria-hidden="true"
                    />
                    Reading downloads from the engine…
                  </p>
                </td>
              </tr>
            ) : visible.length === 0 ? (
              <tr>
                <td colSpan={DOWNLOAD_COLUMNS.length} className="px-6 py-16">
                  <div className="mx-auto flex max-w-sm flex-col items-center text-center">
                    <span className="grid size-12 place-items-center rounded-full border border-chrome-line bg-chrome-panel">
                      <Inbox
                        className="size-5 text-chrome-muted"
                        aria-hidden="true"
                      />
                    </span>
                    <h2 className="mt-3 text-sm font-medium text-chrome-menu-text">
                      {downloads.length === 0
                        ? "No downloads yet"
                        : `Nothing in ${filterLabel}`}
                    </h2>
                    <p className="mt-1 text-xs text-chrome-muted">
                      {downloads.length === 0
                        ? "Add a file address and it will appear here with its real size, speed and progress."
                        : "This category is empty. Show everything, or add another download."}
                    </p>
                    <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
                      <button
                        type="button"
                        onClick={onAddDownload}
                        className="inline-flex items-center gap-1.5 rounded-chrome bg-chrome-menu-active px-3 py-1.5 text-xs text-chrome-title-text hover:bg-chrome-popup-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
                      >
                        <Plus className="size-3.5" aria-hidden="true" />
                        Add download
                      </button>
                      {filter !== "all" ? (
                        <button
                          type="button"
                          onClick={() => setFilter("all")}
                          className="rounded-chrome border border-chrome-line px-3 py-1.5 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
                        >
                          Show all downloads
                        </button>
                      ) : null}
                    </div>
                  </div>
                </td>
              </tr>
            ) : (
              visible.map((item) => (
                <DownloadRow
                  key={item.id}
                  item={item}
                  columnCount={DOWNLOAD_COLUMNS.length}
                  selected={selected.includes(item.id)}
                  expanded={expanded.includes(item.id)}
                  onSelect={(additive) => toggleSelect(item.id, additive)}
                  onToggleExpand={() => toggleExpanded(item.id)}
                  onOpenMenu={(at, trigger) => openMenu(item, at, trigger)}
                />
              ))
            )}
          </tbody>
        </table>
      </div>

      {menu ? (
        <DownloadContextMenu
          item={menu.item}
          at={menu.at}
          onClose={closeMenu}
        />
      ) : null}
    </section>
  );
}
