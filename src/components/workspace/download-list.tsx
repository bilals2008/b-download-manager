import { useEffect, useRef, useState } from "react";

import type { MenuCloseReason } from "@/components/shell/menu-popup";
import {
  EmptyDownloadsArt,
  WarningMark,
} from "@/components/icons/desktop-icons";
import {
  ChevronDown,
  ChevronUp,
  ChevronsUpDown,
  CloseGlyph,
  PlusGlyph,
  SpinnerGlyph,
} from "@/components/icons/ui-icons";

import { DOWNLOAD_COLUMNS } from "@/components/workspace/download-columns";
import { DownloadContextMenu } from "@/components/workspace/download-context-menu";
import { DeleteFileDialog } from "@/components/workspace/delete-file-dialog";
import { DownloadFailedDialog } from "@/components/workspace/download-failed-dialog";
import { DownloadPropertiesDialog } from "@/components/workspace/download-properties-dialog";
import { DownloadRow } from "@/components/workspace/download-row";
import { FindBar } from "@/components/workspace/find-bar";
import { SelectionCheckbox } from "@/components/workspace/selection-checkbox";
import { StatusFilterControl } from "@/components/workspace/status-filter";
import { useColumnWidths } from "@/components/workspace/use-column-widths";
import {
  STATUS_FILTER_LABELS,
  splitPath,
  type Download,
  type SortKey,
} from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import {
  chooseFolder,
  openProgressWindow,
  relocateDownload,
  retryDownload,
} from "@/lib/ipc";
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
    cursor,
    selectRow,
    selectAll,
    clearSelection,
    sort,
    sortBy,
    filter,
    setFilter,
    statusFilter,
    setStatusFilter,
    loading,
    error,
    dismissError,
    reportError,
    reload,
    findOpen,
    closeFind,
    applyToSelection,
  } = useDownloads();
  // The box the rows scroll in, so arrow keys can follow the selection and
  // Find Next can bring its match into view.
  const viewportRef = useRef<HTMLDivElement>(null);
  const { widths, tableRef, beginResize, moveResize, endResize, resizeBy, resetWidths } =
    useColumnWidths(DOWNLOAD_COLUMNS);
  // The row whose menu is open, and where the pointer was.
  const [menu, setMenu] = useState<{
    item: Download;
    at: { x: number; y: number };
  } | null>(null);
  // The control that opened the menu, so the focus goes back to it.
  const menuTrigger = useRef<HTMLElement | null>(null);
  // The id of the row whose properties window is open, so the window keeps up
  // with the progress of the download while it is on screen.
  const [propertiesId, setPropertiesId] = useState<number | null>(null);
  const properties =
    downloads.find((download) => download.id === propertiesId) ?? null;
  // The finished row whose file the user asked to delete from the disk.
  const [deleteFileId, setDeleteFileId] = useState<number | null>(null);
  const deleteTarget =
    downloads.find((download) => download.id === deleteFileId) ?? null;
  // The row whose failure window is open, and the state of the last recovery
  // command it sent.
  const [failureId, setFailureId] = useState<number | null>(null);
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const failure = downloads.find((download) => download.id === failureId) ?? null;

  function openMenu(
    item: Download,
    at: { x: number; y: number },
    trigger: HTMLElement | null,
  ) {
    menuTrigger.current = trigger;
    // A right click acts on the row under the pointer, so that row joins the
    // selection when it was not in it.
    if (!selected.includes(item.id)) {
      selectRow(item.id, "replace");
    }
    setMenu({ item, at });
  }

  function closeMenu(reason?: MenuCloseReason) {
    setMenu(null);
    // A click somewhere else moves the focus where the user clicked, so only
    // a keyboard close gives it back to the row.
    if (reason !== "pointer") {
      menuTrigger.current?.focus();
    }
  }

  /** Open the window that shows this download in flight, the way IDM does. */
  function openProgress(item: Download) {
    openProgressWindow(item.id).catch((failure: unknown) => {
      reportError(describeError(failure));
    });
  }

  function openFailure(item: Download) {
    setRecoveryError(null);
    setFailureId(item.id);
  }

  function closeFailure() {
    setFailureId(null);
    setRecoveryError(null);
    setRecoveryBusy(false);
  }

  /** Retry from the bytes on disk, or start the transfer over. */
  async function retryFailure(restart: boolean) {
    if (!failure) {
      return;
    }
    setRecoveryBusy(true);
    setRecoveryError(null);
    try {
      await retryDownload(failure.id, restart);
      closeFailure();
    } catch (problem) {
      setRecoveryError(describeError(problem));
    } finally {
      setRecoveryBusy(false);
    }
  }

  /** Move the parts and the manifest, then continue from them. */
  async function moveFailure() {
    if (!failure) {
      return;
    }
    let folder: string | null;
    try {
      folder = await chooseFolder(splitPath(failure.dest).dir);
    } catch (problem) {
      setRecoveryError(describeError(problem));
      return;
    }
    if (!folder) {
      return;
    }
    setRecoveryBusy(true);
    setRecoveryError(null);
    try {
      await relocateDownload(failure.id, folder);
      closeFailure();
    } catch (problem) {
      setRecoveryError(describeError(problem));
    } finally {
      setRecoveryBusy(false);
    }
  }

  // The row the keyboard is on: bring it into view when a key, Find Next or a
  // click moves it.
  useEffect(() => {
    if (cursor === null) {
      return;
    }
    viewportRef.current
      ?.querySelector(`[data-download-id="${cursor}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  /** Arrow keys walk the rows the user can see; Shift extends the range. */
  function moveCursor(to: "up" | "down" | "first" | "last", extend: boolean) {
    if (visible.length === 0) {
      return;
    }
    const at =
      cursor === null ? -1 : visible.findIndex((item) => item.id === cursor);
    let next: number;
    if (to === "first") {
      next = 0;
    } else if (to === "last") {
      next = visible.length - 1;
    } else if (to === "down") {
      next = at < 0 ? 0 : Math.min(at + 1, visible.length - 1);
    } else {
      next = at < 0 ? visible.length - 1 : Math.max(at - 1, 0);
    }
    selectRow(visible[next].id, extend ? "range" : "replace");
  }

  const filterLabel =
    filter === "all"
      ? "All Downloads"
      : filter === "unfinished"
        ? "Unfinished"
        : filter === "finished"
          ? "Finished"
          : filter;
  const shownLabel =
    statusFilter === "all"
      ? filterLabel
      : `${filterLabel} · ${STATUS_FILTER_LABELS[statusFilter]}`;

  const picked = visible.filter((item) => selected.includes(item.id)).length;
  const allSelected = visible.length > 0 && picked === visible.length;
  const someSelected = picked > 0 && !allSelected;

  return (
    <section
      aria-label="Downloads"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-chrome-panel"
    >
      {error ? (
        <div
          role="alert"
          className="mx-2 mt-2 flex items-start gap-2 rounded-lg bg-chrome-canvas px-3 py-2 text-xs text-chrome-menu-text"
        >
          <WarningMark className="mt-0.5 size-4 shrink-0" />
          <p className="min-w-0 flex-1 break-words">{error}</p>
          <button
            type="button"
            onClick={reload}
            className="shrink-0 rounded-chrome bg-chrome-panel px-2 py-0.5 text-xs hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Retry
          </button>
          <button
            type="button"
            aria-label="Dismiss error"
            onClick={dismissError}
            className="grid size-5 shrink-0 place-items-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <CloseGlyph className="size-3.5" />
          </button>
        </div>
      ) : null}

      {findOpen ? (
        <FindBar
          onClose={() => {
            closeFind();
            viewportRef.current?.focus();
          }}
        />
      ) : null}

      <div className="flex shrink-0 items-center justify-end border-b border-chrome-line px-3 py-1.5">
        <StatusFilterControl />
      </div>

      <div
        ref={viewportRef}
        tabIndex={0}
        aria-label="Download list"
        className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto scrollbar-thin focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        onClick={(event) => {
          // A click on the empty space under the rows drops the selection.
          if (event.target === event.currentTarget) {
            clearSelection();
          }
        }}
        onKeyDown={(event) => {
          // A header control that already took the key keeps it.
          if (event.defaultPrevented) {
            return;
          }
          if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
            event.preventDefault();
            selectAll();
          } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            moveCursor(event.key === "ArrowDown" ? "down" : "up", event.shiftKey);
          } else if (event.key === "Home" || event.key === "End") {
            event.preventDefault();
            moveCursor(event.key === "Home" ? "first" : "last", event.shiftKey);
          } else if (event.key === "Delete") {
            event.preventDefault();
            applyToSelection("delete");
          } else if (event.key === "Escape") {
            event.preventDefault();
            clearSelection();
          }
        }}
      >
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
                      "sticky top-0 z-10 bg-chrome-panel px-3 py-2 text-chrome-micro font-medium tracking-wide text-chrome-muted",
                      column.align === "right" ? "text-right" : "text-left",
                    )}
                  >
                    {column.id === "name" && visible.length > 0 ? (
                      <SelectionCheckbox
                        className="mr-1.5 inline-flex align-middle"
                        checked={allSelected}
                        indeterminate={someSelected}
                        label={
                          allSelected
                            ? "Clear selection"
                            : "Select all downloads"
                        }
                        onToggle={() =>
                          allSelected ? clearSelection() : selectAll()
                        }
                      />
                    ) : null}

                    {column.sortKey ? (
                      <button
                        type="button"
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
                      className="absolute inset-y-0 right-0 z-20 w-2 cursor-col-resize touch-none after:absolute after:inset-y-1 after:right-0 after:w-px after:bg-transparent hover:after:bg-chrome-focus focus-visible:outline-none focus-visible:after:bg-chrome-focus"
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
                    <SpinnerGlyph className="size-4 motion-safe:animate-spin" />
                    Reading downloads from the engine…
                  </p>
                </td>
              </tr>
            ) : visible.length === 0 ? (
              <tr>
                <td colSpan={DOWNLOAD_COLUMNS.length} className="px-6 py-16">
                  <div className="mx-auto flex max-w-sm flex-col items-center text-center">
                    <EmptyDownloadsArt className="size-14" />
                    <h2 className="mt-3 text-sm font-medium text-chrome-menu-text">
                      {downloads.length === 0
                        ? "No downloads yet"
                        : `Nothing matches ${shownLabel}`}
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
                        className="inline-flex items-center gap-1.5 rounded-chrome bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
                      >
                        <PlusGlyph className="size-3.5" />
                        Add download
                      </button>
                      {filter !== "all" || statusFilter !== "all" ? (
                        <button
                          type="button"
                          onClick={() => {
                            setFilter("all");
                            setStatusFilter("all");
                          }}
                          className="rounded-chrome bg-chrome-canvas px-3 py-1.5 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
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
                  selected={selected.includes(item.id)}
                  onToggle={() => selectRow(item.id, "toggle")}
                  onSelect={(mode) => {
                    selectRow(item.id, mode);
                    // A click moves the keyboard into the list, so the arrow
                    // keys have somewhere to start from.
                    viewportRef.current?.focus({ preventScroll: true });
                  }}
                  onOpenMenu={(at, trigger) => openMenu(item, at, trigger)}
                  onOpen={() => openProgress(item)}
                  onOpenFailure={() => openFailure(item)}
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
          onShowProperties={(item) => setPropertiesId(item.id)}
          onShowFailure={openFailure}
          onDeleteFile={(item) => setDeleteFileId(item.id)}
          onClose={closeMenu}
        />
      ) : null}

      {/* A retry from anywhere closes the window by itself: it is only about
          the failure, and the failure is over. */}
      {failure && failure.status === "error" ? (
        <DownloadFailedDialog
          key={failure.id}
          item={failure}
          busy={recoveryBusy}
          actionError={recoveryError}
          onClose={closeFailure}
          onRetry={(restart) => void retryFailure(restart)}
          onChooseFolder={() => void moveFailure()}
        />
      ) : null}

      {/* Keyed by row, so opening another download starts from its own values
          instead of the drafts the last window left behind. */}
      {properties ? (
        <DownloadPropertiesDialog
          key={properties.id}
          item={properties}
          onClose={() => setPropertiesId(null)}
        />
      ) : null}

      {deleteTarget ? (
        <DeleteFileDialog
          key={deleteTarget.id}
          item={deleteTarget}
          onClose={() => setDeleteFileId(null)}
        />
      ) : null}
    </section>
  );
}
