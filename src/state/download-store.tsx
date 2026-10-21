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
  onDownloadUpdate,
  pauseDownload,
  removeDownload,
  resumeDownload,
  startDownload,
} from "@/lib/ipc";
import { describeError, isDesktop } from "@/lib/errors";
import type { DownloadInfo, DownloadStatus as EngineStatus } from "@/types";

/** What a bulk action does to the selected rows. */
export type DownloadAction = "resume" | "pause" | "stop" | "delete";

interface WorkspaceState {
  downloads: Download[];
  filter: DownloadFilter;
  /** Ids of the selected rows. Empty means nothing is selected. */
  selected: number[];
  sort: SortState;
}

interface SpeedSample {
  downloaded: number;
  at: number;
  speed: number;
}

export interface DownloadStore {
  downloads: Download[];
  filter: DownloadFilter;
  selected: number[];
  sort: SortState;
  /** Rows after the categories pane filter and the sort order are applied. */
  visible: Download[];
  /** Row count per categories pane entry. */
  counts: Record<string, number>;
  /** True while the first list request is in flight. */
  loading: boolean;
  /** Engine or action error, shown above the list. */
  error: string | null;
  dismissError: () => void;
  reload: () => void;
  setFilter: (filter: DownloadFilter) => void;
  toggleSelect: (id: number, additive: boolean) => void;
  selectAll: () => void;
  clearSelection: () => void;
  sortBy: (key: SortKey) => void;
  applyToSelection: (action: DownloadAction) => void;
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
    selected: [],
    sort: { key: "order", dir: "asc" },
  }));
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  const reload = useCallback(() => {
    void load();
  }, [load]);

  const visible = useMemo(
    () => sortDownloads(filterDownloads(state.downloads, state.filter), state.sort),
    [state.downloads, state.filter, state.sort],
  );

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
    );
  }

  const store: DownloadStore = {
    downloads: state.downloads,
    filter: state.filter,
    selected: state.selected,
    sort: state.sort,
    visible,
    counts,
    loading,
    error,
    dismissError: () => setError(null),
    reload,
    setFilter: (filter) =>
      setState((current) => ({
        ...current,
        filter,
        selected: current.selected.filter((id) =>
          filterDownloads(current.downloads, filter).some(
            (item) => item.id === id,
          ),
        ),
      })),
    toggleSelect: (id, additive) =>
      setState((current) => {
        if (!additive) {
          return { ...current, selected: [id] };
        }
        const has = current.selected.includes(id);
        return {
          ...current,
          selected: has
            ? current.selected.filter((value) => value !== id)
            : [...current.selected, id],
        };
      }),
    selectAll: () =>
      setState((current) => ({
        ...current,
        selected: visible.map((item) => item.id),
      })),
    clearSelection: () => setState((current) => ({ ...current, selected: [] })),
    sortBy: (key) =>
      setState((current) => ({
        ...current,
        sort:
          current.sort.key === key
            ? { key, dir: current.sort.dir === "asc" ? "desc" : "asc" }
            : { key, dir: "asc" },
      })),
    applyToSelection: (action) => {
      const ids = latest.current.selected;
      const items = latest.current.downloads.filter((item) =>
        ids.includes(item.id),
      );
      if (items.length === 0) {
        return;
      }

      if (action === "delete") {
        setState((current) => ({
          ...current,
          downloads: current.downloads.map((item) =>
            ids.includes(item.id)
              ? { ...item, status: "stopped", speed: 0, eta: 0 }
              : item,
          ),
          selected: [],
        }));
        run(
          Promise.all(
            items.map(async (item) => {
              try {
                await cancelDownload(item.id);
              } catch {
                // A finished download has no task left to cancel.
              }
              await removeDownload(item.id);
              forget(item.id);
            }),
          ),
        );
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
        } else if (item.status === "stopped" || item.status === "error") {
          // The task is gone, so resuming cannot work: start it again.
          run(restart(item));
        } else {
          run(resumeDownload(item.id));
        }
      }
    },
    addDownload: async (input) => {
      const dest = joinPath(input.savePath, input.fileName);
      const id = await startDownload(input.url, dest, DEFAULT_CONNECTIONS);
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
        connections: DEFAULT_CONNECTIONS,
        error: null,
      };
      setState((current) => ({
        ...current,
        downloads: current.downloads.some((item) => item.id === id)
          ? current.downloads
          : [...current.downloads, row],
      }));
    },
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
      const items = latest.current.downloads.filter(isFinished);
      if (items.length === 0) {
        return;
      }
      setState((current) => ({
        ...current,
        selected: current.selected.filter(
          (id) => !items.some((item) => item.id === id),
        ),
      }));
      run(
        Promise.all(
          items.map(async (item) => {
            await removeDownload(item.id);
            forget(item.id);
          }),
        ),
      );
    },
    startQueue: () => {
      const resumable = latest.current.downloads.filter(
        (item) => item.status === "paused" || item.status === "queued",
      );
      const restartable = latest.current.downloads.filter(
        (item) => item.status === "stopped",
      );
      if (resumable.length === 0 && restartable.length === 0) {
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
      run(
        Promise.all([
          ...resumable.map((item) => resumeDownload(item.id)),
          ...restartable.map((item) => restart(item)),
        ]),
      );
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

  /** Drop a row locally once the engine has forgotten it. */
  function forget(id: number) {
    removed.current.add(id);
    samples.current.delete(id);
    setState((current) => ({
      ...current,
      downloads: current.downloads.filter((item) => item.id !== id),
      selected: current.selected.filter((value) => value !== id),
    }));
  }

  return (
    <DownloadContext.Provider value={store}>{children}</DownloadContext.Provider>
  );
}

function filterDownloads(
  downloads: Download[],
  filter: DownloadFilter,
): Download[] {
  switch (filter) {
    case "all":
      return downloads;
    case "finished":
      return downloads.filter(isFinished);
    case "unfinished":
      return downloads.filter(isUnfinished);
    default:
      return downloads.filter((item) => item.category === (filter as CategoryName));
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
