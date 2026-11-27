import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import type { BatchAsking } from "@/lib/conflict";
import { isFinished } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { nextMatch } from "@/lib/find";
import {
  cancelDownload,
  listDownloads,
  onBridgeDownload,
  onDownloadUpdate,
  pauseDownload,
  removeDownload,
  resumeDownload,
  startQueue,
  stopQueue,
} from "@/lib/ipc";
import {
  torrentPause,
  torrentRemove,
  torrentResume,
} from "@/lib/ipc-torrent";
import {
  mediaCancel as cancelMedia,
  mediaForget,
  mediaResume as resumeMedia,
} from "@/lib/media";
import { rangeIds } from "@/lib/selection";
import type { DownloadInfo } from "@/types";

import { useBulkActions } from "./bulk";
import { useCategories } from "./categories";
import {
  MEDIA_ROW_BASE,
  countByCategory,
  filterDownloads,
  findMatchesFor,
  foldersByCategory,
  matchesQuery,
  sortDownloads,
} from "./list";
import { useMediaRows } from "./media";
import { useCompletionSound } from "../use-completion-sound";
import { useRowMapping } from "./rows";
import { useStartPath } from "./start";
import { useTorrentRows } from "./torrents";
import type { AddOutcome, DownloadStore, WorkspaceState } from "./types";

export type { AddOutcome, NewDownload, SelectMode } from "./types";

export const DownloadContext = createContext<DownloadStore | null>(null);

export function useDownloads(): DownloadStore {
  const store = useContext(DownloadContext);
  if (!store) {
    throw new Error("useDownloads must be used inside <DownloadProvider>");
  }
  return store;
}

export function DownloadProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<WorkspaceState>(() => ({
    downloads: [],
    filter: "all",
    statusFilter: "all",
    filterPath: null,
    selected: [],
    anchor: null,
    cursor: null,
    pendingDelete: null,
    sort: { key: "order", dir: "asc" },
    find: "",
    findOpen: false,
    matchId: null,
  }));
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  /** A media row the reader removed stays gone for the session. */
  const dismissedMedia = useRef(new Set<string>());
  // Cancelled tasks can emit one last update after the row was removed. Their
  // ids are never reused, so a tombstone set is enough to keep them out.
  const removed = useRef(new Set<number>());
  const latest = useRef(state);
  useEffect(() => {
    latest.current = state;
  }, [state]);

  // Every finished download is announced once, media rows included: they
  // arrive in the same list.
  useCompletionSound(state.downloads);

  const run = useCallback((task: Promise<unknown>) => {
    task.catch((failure: unknown) => setError(describeError(failure)));
  }, []);

  const {
    categories,
    categoriesRef,
    addCategory,
    updateCategory,
    removeCategory,
    assignCategory,
    updateMeta,
    changeDestination,
    deleteFile,
    queue,
    dequeue,
  } = useCategories({
    latest,
    setState,
    run,
    forget,
    dismissMedia: (url) => dismissedMedia.current.add(url),
  });

  const { samples, sampleFor, toDownload } = useRowMapping({ categoriesRef });

  const media = useMediaRows(setState, sampleFor, dismissedMedia, categoriesRef);
  useTorrentRows(setState, categoriesRef);

  // A link that the browser extension handed over.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let dispose: (() => void) | undefined;
    let closed = false;
    void onBridgeDownload((payload) => {
      void startPath.takeFromBrowser(payload);
    }).then((unlisten) => {
      if (closed) {
        unlisten();
      } else {
        dispose = unlisten;
      }
    });
    return () => {
      closed = true;
      dispose?.();
    };
  }, []);

  // The engine sends a report for every running download several times a
  // second. Every report redrew the whole list, so a window with a few
  // downloads running spent its time in the renderer and stopped answering.
  // The reports are collected here and put into the state once per frame:
  // the newest report of each row is drawn, the ones before it are dropped.
  const pending = useRef(new Map<number, DownloadInfo>());
  const frame = useRef(0);

  const applyInfo = useCallback(
    (info: DownloadInfo) => {
      if (removed.current.has(info.id)) {
        return;
      }
      pending.current.set(info.id, info);
      if (frame.current) {
        return;
      }
      frame.current = window.requestAnimationFrame(() => {
        frame.current = 0;
        const reports = [...pending.current.values()];
        pending.current.clear();
        if (!reports.length) {
          return;
        }
        const known = latest.current.downloads;
        const rows = reports.map((report) => ({
          id: report.id,
          next: toDownload(
            report,
            known.find((item) => item.id === report.id),
          ),
        }));
        setState((current) => {
          // An event can arrive before the effect above mirrors the newest
          // state, so look again inside the updater and never append a
          // second row.
          const downloads = current.downloads.slice();
          for (const row of rows) {
            if (removed.current.has(row.id)) {
              continue;
            }
            const index = downloads.findIndex((item) => item.id === row.id);
            if (index < 0) {
              downloads.push(row.next);
            } else {
              downloads[index] = {
                ...row.next,
                addedAt: downloads[index].addedAt,
              };
            }
          }
          return { ...current, downloads };
        });
      });
    },
    [toDownload],
  );

  const load = useCallback(async () => {
    if (!isDesktop()) {
      setLoading(false);
      setError(describeError(null));
      return;
    }
    setLoading(true);
    try {
      const infos = (await listDownloads()).filter(
        (info) => !removed.current.has(info.id),
      );
      const known = latest.current.downloads;
      const downloads = infos.map((info) =>
        toDownload(info, known.find((item) => item.id === info.id)),
      );
      for (const id of samples.current.keys()) {
        if (!infos.some((info) => info.id === id)) {
          samples.current.delete(id);
        }
      }
      setState((current) => {
        // The media rows are a list of their own, polled beside this one. A
        // reload that lands after their poll must not wipe them: the engine
        // rows are replaced, the media rows stay until their own poll says
        // otherwise.
        const kept = current.downloads.filter(
          (item) => item.mediaJob !== undefined || item.torrent !== undefined,
        );
        const merged = [...downloads, ...kept];
        return {
          ...current,
          downloads: merged,
          selected: current.selected.filter((id) =>
            merged.some((item) => item.id === id),
          ),
        };
      });
      setError(null);
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setLoading(false);
    }
  }, [toDownload]);

  useEffect(() => {
    void load();
    if (!isDesktop()) {
      return;
    }
    let dispose: (() => void) | undefined;
    let closed = false;
    void onDownloadUpdate(applyInfo).then((unlisten) => {
      if (closed) {
        unlisten();
      } else {
        dispose = unlisten;
      }
    });
    return () => {
      closed = true;
      dispose?.();
    };
  }, [applyInfo, load]);

  const reload = useCallback(() => {
    void load();
  }, [load]);

  const visible = useMemo(
    () =>
      sortDownloads(
        filterDownloads(
          state.downloads,
          state.filter,
          state.filterPath,
          state.statusFilter,
        ),
        state.sort,
      ),
    [
      state.downloads,
      state.filter,
      state.filterPath,
      state.statusFilter,
      state.sort,
    ],
  );

  const findMatches = useMemo(
    () => findMatchesFor(visible, state.find),
    [state.find, visible],
  );
  const findIndex =
    state.matchId === null ? -1 : findMatches.indexOf(state.matchId);

  const counts = useMemo(
    () => countByCategory(categories, state.downloads),
    [categories, state.downloads],
  );

  const folders = useMemo(
    () => foldersByCategory(categories, state.downloads),
    [categories, state.downloads],
  );

  const startPath = useStartPath({ setState, forget });
  const bulk = useBulkActions({ latest, setState, run });

  const store: DownloadStore = {
    downloads: state.downloads,
    categories,
    filter: state.filter,
    statusFilter: state.statusFilter,
    setStatusFilter: (statusFilter) =>
      setState((current) => ({
        ...current,
        statusFilter,
        selected: current.selected.filter((id) =>
          filterDownloads(
            current.downloads,
            current.filter,
            current.filterPath,
            statusFilter,
          ).some((item) => item.id === id),
        ),
      })),
    addCategory,
    updateCategory,
    removeCategory,
    assignCategory,
    updateMeta,
    changeDestination,
    deleteFile,
    queue,
    dequeue,
    filterPath: state.filterPath,
    selected: state.selected,
    cursor: state.cursor,
    sort: state.sort,
    visible,
    find: state.find,
    findOpen: state.findOpen,
    findMatches,
    findIndex,
    counts,
    folders,
    browserRequest: startPath.browserRequest,
    clearBrowserRequest: startPath.clearBrowserRequest,
    loading,
    error,
    dismissError: () => setError(null),
    reportError: (message: string) => setError(message),
    reload,
    setFilter: (filter) =>
      setState((current) => ({
        ...current,
        filter,
        filterPath: null,
        selected: current.selected.filter((id) =>
          filterDownloads(
            current.downloads,
            filter,
            null,
            current.statusFilter,
          ).some((item) => item.id === id),
        ),
      })),
    setFolder: (category, path) =>
      setState((current) => ({
        ...current,
        filter: category,
        filterPath: path,
        selected: current.selected.filter((id) =>
          filterDownloads(
            current.downloads,
            category,
            path,
            current.statusFilter,
          ).some((item) => item.id === id),
        ),
      })),
    selectRow: (id, mode) =>
      setState((current) => {
        if (mode === "replace") {
          return { ...current, selected: [id], anchor: id, cursor: id };
        }
        if (mode === "toggle") {
          const has = current.selected.includes(id);
          return {
            ...current,
            selected: has
              ? current.selected.filter((value) => value !== id)
              : [...current.selected, id],
            anchor: has ? current.anchor : id,
            cursor: id,
          };
        }
        // Shift picks the range in the order the list shows, not the id order.
        const ids = sortDownloads(
          filterDownloads(
            current.downloads,
            current.filter,
            current.filterPath,
            current.statusFilter,
          ),
          current.sort,
        ).map((row) => row.id);
        const picked = rangeIds(ids, current.anchor, id);
        const kept = current.anchor !== null && ids.includes(current.anchor);
        const next =
          mode === "add-range"
            ? [...new Set([...current.selected, ...picked])]
            : picked;
        return {
          ...current,
          selected: next,
          anchor: kept ? current.anchor : id,
          cursor: id,
        };
      }),
    selectAll: () =>
      setState((current) => ({
        ...current,
        selected: visible.map((item) => item.id),
      })),
    selectByFilter: (filter) =>
      setState((current) => ({
        ...current,
        selected: filterDownloads(
          current.downloads,
          filter,
          null,
          current.statusFilter,
        ).map((item) => item.id),
      })),
    clearSelection: () =>
      setState((current) => ({
        ...current,
        selected: [],
        anchor: null,
        cursor: null,
      })),
    sortBy: (key) =>
      setState((current) => ({
        ...current,
        sort:
          current.sort.key === key
            ? { key, dir: current.sort.dir === "asc" ? "desc" : "asc" }
            : { key, dir: "asc" },
      })),
    setFind: (text) =>
      setState((current) => ({ ...current, find: text, matchId: null })),
    openFind: () => setState((current) => ({ ...current, findOpen: true })),
    // The query is kept, so F3 after closing the bar repeats the search.
    closeFind: () => setState((current) => ({ ...current, findOpen: false })),
    findNext: (step) =>
      setState((current) => {
        const query = current.find.trim().toLowerCase();
        // A closed bar has no query in it; the first press only brings it up.
        if (!query) {
          return { ...current, findOpen: true };
        }
        const ids = sortDownloads(
          filterDownloads(
            current.downloads,
            current.filter,
            current.filterPath,
            current.statusFilter,
          ),
          current.sort,
        )
          .filter((item) => matchesQuery(item, query))
          .map((item) => item.id);
        const id = nextMatch(ids, current.matchId, step);
        return id === null
          ? { ...current, findOpen: true }
          : {
              ...current,
              findOpen: true,
              matchId: id,
              selected: [id],
              anchor: id,
              cursor: id,
            };
      }),
    applyToSelection: (action) => {
      const ids = latest.current.selected;
      const items = latest.current.downloads.filter((item) =>
        ids.includes(item.id),
      );

      if (items.length === 0) {
        return;
      }

      if (action === "delete") {
        // A batch is worth a confirmation; one row deletes the way a list does.
        if (ids.length > 1) {
          setState((current) => ({ ...current, pendingDelete: ids }));
          return;
        }
        removeMany(ids);
        return;
      }

      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) => {
          if (!ids.includes(item.id) || item.status === "completed") {
            return item;
          }
          if (item.mediaJob) {
            if (action === "stop") {
              return { ...item, status: "stopped", speed: 0, eta: 0 };
            }
            if (action === "resume" && item.mediaCanResume) {
              return {
                ...item,
                status: "downloading",
                speed: 0,
                eta: 0,
                error: null,
              };
            }
            return item;
          }
          if (action === "pause") {
            return { ...item, status: "paused", speed: 0, eta: 0 };
          }
          if (action === "stop") {
            return { ...item, status: "stopped", speed: 0, eta: 0 };
          }
          return { ...item, status: "downloading", speed: 0, error: null };
        }),
      }));

      for (const item of items) {
        if (item.status === "completed") {
          continue;
        }
        if (item.mediaJob) {
          if (action === "stop") {
            run(cancelMedia(item.mediaJob));
          } else if (action === "resume" && item.mediaCanResume) {
            run(resumeMedia(item.mediaJob));
          }
          continue;
        }
        // A torrent is held and let go by the torrent client, not by the
        // engine's download commands.
        if (item.torrent) {
          run(
            action === "resume"
              ? torrentResume(item.torrent)
              : torrentPause(item.torrent),
          );
          continue;
        }
        if (action === "pause") {
          run(pauseDownload(item.id));
        } else if (action === "stop") {
          run(cancelDownload(item.id));
        } else {
          // Resuming a stopped or failed row is not a new download: the engine
          // still has its parts and manifest, and picks up from them.
          run(resumeDownload(item.id));
        }
      }
    },
    pendingDelete: state.pendingDelete,
    confirmDelete: () => {
      const ids =
        latest.current.pendingDelete?.filter((id) =>
          latest.current.downloads.some((item) => item.id === id),
        ) ?? [];
      if (ids.length > 0) {
        removeMany(ids);
      } else {
        setState((current) => ({ ...current, pendingDelete: null }));
      }
    },
    cancelDelete: () =>
      setState((current) => ({ ...current, pendingDelete: null })),
    addDownload: (input) => startPath.beginDownload(input),
    conflict: startPath.conflict,
    answerConflict: (answer, applyToRemaining) =>
      startPath.settleConflict({ choice: answer, applyToRemaining }),
    cancelConflict: () => startPath.settleConflict(null),
    pauseAll: bulk.pauseAll,
    resumeAll: bulk.resumeAll,
    stopAll: bulk.stopAll,
    redownloadSelected: () => {
      // A live row has to be stopped before it can start over, so it is left
      // out instead of answering with an error nobody asked for.
      const items = latest.current.downloads.filter(
        (item) =>
          latest.current.selected.includes(item.id) &&
          !item.mediaJob &&
          !item.torrent &&
          item.status !== "downloading" &&
          item.status !== "queued",
      );
      if (items.length === 0) {
        return;
      }
      setState((current) => ({ ...current, selected: [] }));
      // One at a time, so the File Exists question of a batch is asked about
      // one file and answered for all of them, instead of several questions
      // racing for the same dialog.
      const batch: BatchAsking = { remaining: 0, choice: null };
      void (async () => {
        for (let index = 0; index < items.length; index += 1) {
          batch.remaining = items.length - index - 1;
          let outcome: AddOutcome;
          try {
            outcome = await startPath.restart(items[index], batch);
          } catch (failure) {
            setError(describeError(failure));
            continue;
          }
          if (outcome === "cancelled") {
            return;
          }
        }
      })();
    },
    deleteCompleted: () => {
      const ids = latest.current.downloads
        .filter((item) => isFinished(item) && !item.mediaJob && !item.torrent)
        .map((item) => item.id);
      if (ids.length === 0) {
        return;
      }
      // The command names a group, not the row the user is looking at, so it
      // always asks - even when only one row is finished.
      setState((current) => ({ ...current, pendingDelete: ids }));
    },
    startQueue: async () => {
      try {
        await startQueue();
      } catch (failure) {
        setError(describeError(failure));
      }
    },
    stopQueue: async () => {
      try {
        await stopQueue();
      } catch (failure) {
        setError(describeError(failure));
      }
    },
  };

  /** Drop a batch locally and in the engine, once the user has confirmed. */
  function removeMany(ids: number[]) {
    setState((current) => ({
      ...current,
      downloads: current.downloads.map((item) =>
        ids.includes(item.id)
          ? { ...item, status: "stopped", speed: 0, eta: 0 }
          : item,
      ),
      selected: [],
      pendingDelete: null,
    }));
    run(
      Promise.all(
        ids.map(async (id) => {
          const item = latest.current.downloads.find((row) => row.id === id);
          if (item?.mediaJob) {
            dismissedMedia.current.add(item.url);
            await mediaForget(item.mediaJob, false);
            forget(id);
            return;
          }
          // A torrent leaves the list, its files stay where they are.
          if (item?.torrent) {
            await torrentRemove(item.torrent, false);
            forget(id);
            return;
          }
          try {
            await cancelDownload(id);
          } catch {
            // A finished download has no task left to cancel.
          }
          await removeDownload(id);
          forget(id);
        }),
      ),
    );
  }

  /** Drop a row locally once the engine has forgotten it. */
  function forget(id: number) {
    removed.current.add(id);
    samples.current.delete(id);
    if (id >= MEDIA_ROW_BASE) {
      media.mediaAdded.current.delete(id - MEDIA_ROW_BASE);
    }
    setState((current) => ({
      ...current,
      downloads: current.downloads.filter((item) => item.id !== id),
      selected: current.selected.filter((value) => value !== id),
      cursor: current.cursor === id ? null : current.cursor,
    }));
  }

  return (
    <DownloadContext.Provider value={store}>{children}</DownloadContext.Provider>
  );
}
