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

import {
  CATEGORIES,
  categoryOfFile,
  DEFAULT_CONNECTIONS,
  fileNameFromUrl,
  folderLabel,
  isFinished,
  isUnfinished,
  joinPath,
  splitPath,
  type CategoryName,
  type Download,
  type DownloadFilter,
  type SortKey,
  type SortState,
} from "@/lib/downloads";
import {
  cancelDownload,
  listDownloads,
  onBridgeDownload,
  onDownloadUpdate,
  pauseDownload,
  removeDownload,
  resumeDownload,
  startDownload,
  type BridgeDownload,
} from "@/lib/ipc";
import { describeError, isDesktop } from "@/lib/errors";
import { nextMatch } from "@/lib/find";
import { rangeIds } from "@/lib/selection";
import type { DownloadInfo, DownloadStatus as EngineStatus } from "@/types";

/** What a bulk action does to the selected rows. */
export type DownloadAction = "resume" | "pause" | "stop" | "delete";

/** How a click picks rows: plain, Ctrl or Cmd, Shift for a range, or both for a second range. */
export type SelectMode = "replace" | "toggle" | "range" | "add-range";

interface WorkspaceState {
  downloads: Download[];
  filter: DownloadFilter;
  /** Save folder picked in the tree, on top of the category filter. */
  filterPath: string | null;
  /** Ids of the selected rows. Empty means nothing is selected. */
  selected: number[];
  /** The row a Shift click measures its range from. */
  anchor: number | null;
  /** The row the keyboard is on, which the arrows step from. */
  cursor: number | null;
  /** A batch delete waiting for the confirmation window. */
  pendingDelete: number[] | null;
  sort: SortState;
  /** What the Find bar searches for, by file name or address. */
  find: string;
  /** Whether the Find bar is on screen. */
  findOpen: boolean;
  /** The row Find Next landed on. */
  matchId: number | null;
}

/** A save folder under a category in the tree, with what it holds. */
export interface CategoryFolder {
  path: string;
  label: string;
  count: number;
}

interface SpeedSample {
  downloaded: number;
  at: number;
  speed: number;
}

/** A link the browser extension handed over, waiting in the add dialog. */
export interface BrowserRequest {
  url: string;
  name: string;
  /** The page the link was on, so the server accepts the request. */
  referer: string;
}

export interface DownloadStore {
  downloads: Download[];
  filter: DownloadFilter;
  /** The save folder the tree has drilled into, or null for the whole category. */
  filterPath: string | null;
  selected: number[];
  /** The row the keyboard is on, or null. */
  cursor: number | null;
  sort: SortState;
  /** Rows after the categories pane filter and the sort order are applied. */
  visible: Download[];
  /** What the Find bar searches for. */
  find: string;
  findOpen: boolean;
  /** The rows the query matches, in the order the list shows. */
  findMatches: number[];
  /** Where the current match sits in "findMatches", or -1. */
  findIndex: number;
  setFind: (text: string) => void;
  openFind: () => void;
  closeFind: () => void;
  /** Step through the matches; opens the bar when it is closed. */
  findNext: (step: 1 | -1) => void;
  /** Row count per categories pane entry. */
  counts: Record<string, number>;
  /** Save folders per category, for the tree children. */
  folders: Record<CategoryName, CategoryFolder[]>;
  /** A link the browser handed over, until the add dialog takes it. */
  browserRequest: BrowserRequest | null;
  clearBrowserRequest: () => void;
  /** True while the first list request is in flight. */
  loading: boolean;
  /** Engine or action error, shown above the list. */
  error: string | null;
  dismissError: () => void;
  /** Show an error that came from an action outside the list. */
  reportError: (message: string) => void;
  reload: () => void;
  setFilter: (filter: DownloadFilter) => void;
  /** Drill into one save folder of a category. */
  setFolder: (category: CategoryName, path: string) => void;
  /** A click on a row: replace the selection, toggle it, or add a range. */
  selectRow: (id: number, mode: SelectMode) => void;
  selectAll: () => void;
  /** Select every download a category filter matches. */
  selectByFilter: (filter: DownloadFilter) => void;
  clearSelection: () => void;
  sortBy: (key: SortKey) => void;
  applyToSelection: (action: DownloadAction) => void;
  /** A batch delete waiting for the confirmation window. */
  pendingDelete: number[] | null;
  confirmDelete: () => void;
  cancelDelete: () => void;
  addDownload: (input: NewDownload) => Promise<void>;
  pauseAll: () => void;
  stopAll: () => void;
  redownloadSelected: () => void;
  deleteCompleted: () => void;
  startQueue: () => void;
  stopQueue: () => void;
}

export interface NewDownload {
  url: string;
  fileName: string;
  category: CategoryName;
  savePath: string;
  /** The page the link was on, when the browser knew it. */
  referer?: string;
}

const DownloadContext = createContext<DownloadStore | null>(null);

const ENGINE_STATUS: Record<EngineStatus, Download["status"]> = {
  queued: "queued",
  running: "downloading",
  paused: "paused",
  completed: "completed",
  failed: "error",
  cancelled: "stopped",
};

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
  const [browserRequest, setBrowserRequest] = useState<BrowserRequest | null>(
    null,
  );

  // The engine sends absolute counters, so speed is the delta between two
  // events. One sample per download, kept out of state to avoid a render per
  // packet. The exponential smoothing keeps the column from flickering.
  const samples = useRef(new Map<number, SpeedSample>());
  // Cancelled tasks can emit one last update after the row was removed. Their
  // ids are never reused, so a tombstone set is enough to keep them out.
  const removed = useRef(new Set<number>());
  const latest = useRef(state);
  useEffect(() => {
    latest.current = state;
  }, [state]);

  const run = useCallback((task: Promise<unknown>) => {
    task.catch((failure: unknown) => setError(describeError(failure)));
  }, []);

  const sampleFor = useCallback((info: DownloadInfo) => {
    const now = Date.now();
    const previous = samples.current.get(info.id);
    let speed = 0;

    if (info.status === "running") {
      if (previous && now > previous.at && info.downloaded >= previous.downloaded) {
        const instant =
          ((info.downloaded - previous.downloaded) * 1000) / (now - previous.at);
        speed =
          previous.speed > 0 ? previous.speed * 0.65 + instant * 0.35 : instant;
      } else if (previous) {
        speed = previous.speed;
      }
    }

    samples.current.set(info.id, { downloaded: info.downloaded, at: now, speed });
    const remaining = Math.max(0, info.total - info.downloaded);
    return { speed, eta: speed > 0 ? remaining / speed : 0, at: now };
  }, []);

  const toDownload = useCallback(
    (info: DownloadInfo, previous?: Download): Download => {
      const { dir, base } = splitPath(info.dest);
      const fileName = base || fileNameFromUrl(info.url);
      const sample = sampleFor(info);
      return {
        id: info.id,
        fileName,
        url: info.url,
        // shortcut: the engine has no category field, so a category picked in
        // the dialog lives for the session only.
        category: previous?.category ?? categoryOfFile(fileName),
        size: info.total,
        downloaded: info.downloaded,
        status: ENGINE_STATUS[info.status],
        speed: sample.speed,
        eta: sample.eta,
        addedAt: previous?.addedAt ?? sample.at,
        lastTryAt:
          info.status === "running" ? sample.at : (previous?.lastTryAt ?? sample.at),
        savePath: dir,
        dest: info.dest,
        referer: info.referer,
        connections: info.connections,
        error: info.error,
      };
    },
    [sampleFor],
  );

  const applyInfo = useCallback(
    (info: DownloadInfo) => {
      if (removed.current.has(info.id)) {
        return;
      }
      const known = latest.current.downloads.find((item) => item.id === info.id);
      const next = toDownload(info, known);
      setState((current) => {
        // An event can arrive before the effect above mirrors the newest state,
        // so look again inside the updater and never append a second row.
        const existing = current.downloads.find((item) => item.id === info.id);
        const merged = existing
          ? { ...next, category: existing.category, addedAt: existing.addedAt }
          : next;
        return {
          ...current,
          downloads: existing
            ? current.downloads.map((item) =>
                item.id === info.id ? merged : item,
              )
            : [...current.downloads, merged],
        };
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
      setState((current) => ({
        ...current,
        downloads,
        selected: current.selected.filter((id) =>
          downloads.some((item) => item.id === id),
        ),
      }));
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

  // A link that the browser extension handed over.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let dispose: (() => void) | undefined;
    let closed = false;
    void onBridgeDownload((payload) => {
      void takeFromBrowser(payload);
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

  const reload = useCallback(() => {
    void load();
  }, [load]);

  const visible = useMemo(
    () =>
      sortDownloads(
        filterDownloads(state.downloads, state.filter, state.filterPath),
        state.sort,
      ),
    [state.downloads, state.filter, state.filterPath, state.sort],
  );

  const findMatches = useMemo(() => {
    const query = state.find.trim().toLowerCase();
    if (!query) {
      return [];
    }
    return visible
      .filter((item) => matchesQuery(item, query))
      .map((item) => item.id);
  }, [state.find, visible]);

  const findIndex =
    state.matchId === null ? -1 : findMatches.indexOf(state.matchId);

  const counts = useMemo(() => {
    const result: Record<string, number> = { all: 0, unfinished: 0, finished: 0 };
    for (const category of CATEGORIES) {
      result[category] = 0;
    }
    for (const item of state.downloads) {
      result.all += 1;
      if (isFinished(item)) {
        result.finished += 1;
      }
      if (isUnfinished(item)) {
        result.unfinished += 1;
      }
      result[item.category] += 1;
    }
    return result;
  }, [state.downloads]);

  const folders = useMemo(() => {
    const result = {} as Record<CategoryName, CategoryFolder[]>;
    for (const category of CATEGORIES) {
      result[category] = [];
    }
    const paths = new Map<CategoryName, Map<string, number>>();
    for (const item of state.downloads) {
      if (!item.savePath) {
        continue;
      }
      let bucket = paths.get(item.category);
      if (!bucket) {
        bucket = new Map<string, number>();
        paths.set(item.category, bucket);
      }
      bucket.set(item.savePath, (bucket.get(item.savePath) ?? 0) + 1);
    }
    for (const [category, bucket] of paths) {
      result[category] = [...bucket.entries()]
        .map(([path, count]) => ({ path, label: folderLabel(path), count }))
        .sort((left, right) => left.path.localeCompare(right.path));
    }
    return result;
  }, [state.downloads]);

  /** Replace a finished or failed download with a fresh engine job. */
  async function restart(item: Download) {
    try {
      await cancelDownload(item.id);
    } catch {
      // A finished download has no task left to cancel.
    }
    try {
      await removeDownload(item.id);
    } catch {
      // It may already be gone from the engine.
    }
    forget(item.id);
    await startDownload(
      item.url,
      item.dest,
      item.connections || DEFAULT_CONNECTIONS,
      item.referer ?? undefined,
    );
  }

  /** Start one download and put its row in the list. */
  async function beginDownload(input: NewDownload) {
    const dest = joinPath(input.savePath, input.fileName);
    const id = await startDownload(
      input.url,
      dest,
      DEFAULT_CONNECTIONS,
      input.referer,
    );
    const now = Date.now();
    const { dir, base } = splitPath(dest);
    const row: Download = {
      id,
      fileName: base || input.fileName,
      url: input.url,
      category: input.category,
      size: 0,
      downloaded: 0,
      status: "queued",
      speed: 0,
      eta: 0,
      addedAt: now,
      lastTryAt: now,
      savePath: dir,
      dest,
      referer: input.referer?.trim() || null,
      connections: DEFAULT_CONNECTIONS,
      error: null,
    };
    setState((current) => ({
      ...current,
      downloads: current.downloads.some((item) => item.id === id)
        ? current.downloads
        : [...current.downloads, row],
    }));
  }

  /**
   * A link the browser extension handed over.
   *
   * The browser waits while the user confirms it, the way IDM's dialog does:
   * the address and the name the browser gave the file are put in the add
   * dialog, which starts the download when the user says so.
   */
  function takeFromBrowser(payload: BridgeDownload) {
    setBrowserRequest({
      url: payload.url,
      name: splitPath(payload.name).base,
      referer: payload.referer,
    });
  }

  const store: DownloadStore = {
    downloads: state.downloads,
    filter: state.filter,
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
    browserRequest,
    clearBrowserRequest: () => setBrowserRequest(null),
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
          filterDownloads(current.downloads, filter, null).some(
            (item) => item.id === id,
          ),
        ),
      })),
    setFolder: (category, path) =>
      setState((current) => ({
        ...current,
        filter: category,
        filterPath: path,
        selected: current.selected.filter((id) =>
          filterDownloads(current.downloads, category, path).some(
            (item) => item.id === id,
          ),
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
          filterDownloads(current.downloads, current.filter, current.filterPath),
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
        selected: filterDownloads(current.downloads, filter, null).map(
          (item) => item.id,
        ),
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
    closeFind: () =>
      setState((current) => ({ ...current, findOpen: false })),
    findNext: (step) =>
      setState((current) => {
        const query = current.find.trim().toLowerCase();
        // A closed bar has no query in it; the first press only brings it up.
        if (!query) {
          return { ...current, findOpen: true };
        }
        const ids = sortDownloads(
          filterDownloads(current.downloads, current.filter, current.filterPath),
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
    addDownload: (input) => beginDownload(input),
    pauseAll: () => {
      const items = latest.current.downloads.filter(
        (item) => item.status === "downloading",
      );
      if (items.length === 0) {
        return;
      }
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "downloading"
            ? { ...item, status: "paused", speed: 0, eta: 0 }
            : item,
        ),
      }));
      run(Promise.all(items.map((item) => pauseDownload(item.id))));
    },
    stopAll: () => {
      const items = latest.current.downloads.filter(
        (item) => item.status === "downloading",
      );
      if (items.length === 0) {
        return;
      }
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "downloading"
            ? { ...item, status: "stopped", speed: 0, eta: 0 }
            : item,
        ),
      }));
      run(Promise.all(items.map((item) => cancelDownload(item.id))));
    },
    redownloadSelected: () => {
      const items = latest.current.downloads.filter((item) =>
        latest.current.selected.includes(item.id),
      );
      if (items.length === 0) {
        return;
      }
      setState((current) => ({ ...current, selected: [] }));
      run(Promise.all(items.map((item) => restart(item))));
    },
    deleteCompleted: () => {
      const ids = latest.current.downloads
        .filter((item) => isFinished(item))
        .map((item) => item.id);
      if (ids.length === 0) {
        return;
      }
      // The command names a group, not the row the user is looking at, so it
      // always asks - even when only one row is finished.
      setState((current) => ({ ...current, pendingDelete: ids }));
    },
    startQueue: () => {
      const resumable = latest.current.downloads.filter(
        (item) =>
          item.status === "paused" ||
          item.status === "queued" ||
          item.status === "stopped",
      );
      if (resumable.length === 0) {
        return;
      }
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "paused" ||
          item.status === "queued" ||
          item.status === "stopped"
            ? { ...item, status: "downloading", speed: 0, error: null }
            : item,
        ),
      }));
      run(Promise.all(resumable.map((item) => resumeDownload(item.id))));
    },
    stopQueue: () => {
      const items = latest.current.downloads.filter(
        (item) => item.status === "downloading",
      );
      if (items.length === 0) {
        return;
      }
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "downloading"
            ? { ...item, status: "paused", speed: 0, eta: 0 }
            : item,
        ),
      }));
      run(Promise.all(items.map((item) => pauseDownload(item.id))));
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

/** What the Find bar looks at: the file name and the address it came from. */
function matchesQuery(item: Download, query: string): boolean {
  return (
    item.fileName.toLowerCase().includes(query) ||
    item.url.toLowerCase().includes(query)
  );
}

function filterDownloads(
  downloads: Download[],
  filter: DownloadFilter,
  path: string | null,
): Download[] {
  const scoped =
    path === null
      ? downloads
      : downloads.filter((item) => item.savePath === path);
  switch (filter) {
    case "all":
      return scoped;
    case "finished":
      return scoped.filter(isFinished);
    case "unfinished":
      return scoped.filter(isUnfinished);
    default:
      return scoped.filter((item) => item.category === (filter as CategoryName));
  }
}

function sortDownloads(downloads: Download[], sort: SortState): Download[] {
  const factor = sort.dir === "asc" ? 1 : -1;
  return [...downloads].sort((left, right) => {
    switch (sort.key) {
      case "name":
        return left.fileName.localeCompare(right.fileName) * factor;
      case "size":
        return (left.size - right.size) * factor;
      case "status":
        return left.status.localeCompare(right.status) * factor;
      case "eta":
        return (left.eta - right.eta) * factor;
      case "speed":
        return (left.speed - right.speed) * factor;
      case "lastTry":
        return (left.lastTryAt - right.lastTryAt) * factor;
      case "path":
        return left.savePath.localeCompare(right.savePath) * factor;
      default:
        return left.id - right.id;
    }
  });
}
