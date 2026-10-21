import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import {
  CATEGORIES,
  isFinished,
  isUnfinished,
  seedDownloads,
  type CategoryName,
  type Download,
  type DownloadFilter,
  type SortKey,
  type SortState,
} from "@/lib/downloads";

/** What a bulk action does to the selected rows. */
export type DownloadAction = "resume" | "pause" | "stop" | "delete";

interface WorkspaceState {
  downloads: Download[];
  filter: DownloadFilter;
  /** Ids of the selected rows. Empty means nothing is selected. */
  selected: number[];
  sort: SortState;
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
  setFilter: (filter: DownloadFilter) => void;
  toggleSelect: (id: number, additive: boolean) => void;
  selectAll: () => void;
  clearSelection: () => void;
  sortBy: (key: SortKey) => void;
  applyToSelection: (action: DownloadAction) => void;
  addDownload: (input: NewDownload) => void;
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

const DEFAULT_SPEED = 1_572_864; // 1.5 MB/s
const TICK_MS = 500;

export function useDownloads(): DownloadStore {
  const store = useContext(DownloadContext);
  if (!store) {
    throw new Error("useDownloads must be used inside <DownloadProvider>");
  }
  return store;
}

export function DownloadProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<WorkspaceState>(() => ({
    downloads: seedDownloads(),
    filter: "all",
    selected: [],
    sort: { key: "order", dir: "asc" },
  }));

  // The clone has no network engine, so running downloads are advanced here.
  // Everything the UI shows - progress, speed, time left, the counts in the
  // categories pane - follows from this single tick.
  useEffect(() => {
    const timer = window.setInterval(() => {
      setState((current) => {
        if (!current.downloads.some((item) => item.status === "downloading")) {
          return current;
        }
        return {
          ...current,
          downloads: current.downloads.map((item) => {
            if (item.status !== "downloading" || item.speed <= 0) {
              return item;
            }
            const downloaded = Math.min(
              item.size,
              item.downloaded + (item.speed * TICK_MS) / 1000,
            );
            const done = downloaded >= item.size;
            return {
              ...item,
              downloaded,
              status: done ? "completed" : item.status,
              speed: done ? 0 : item.speed,
              eta: done ? 0 : (item.size - downloaded) / item.speed,
            };
          }),
        };
      });
    }, TICK_MS);
    return () => window.clearInterval(timer);
  }, []);

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

  const store: DownloadStore = {
    downloads: state.downloads,
    filter: state.filter,
    selected: state.selected,
    sort: state.sort,
    visible,
    counts,
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
    applyToSelection: (action) =>
      setState((current) => {
        if (current.selected.length === 0) {
          return current;
        }
        if (action === "delete") {
          return {
            ...current,
            downloads: current.downloads.filter(
              (item) => !current.selected.includes(item.id),
            ),
            selected: [],
          };
        }
        return {
          ...current,
          downloads: current.downloads.map((item) => {
            if (!current.selected.includes(item.id)) {
              return item;
            }
            if (action === "resume") {
              return {
                ...item,
                status: "downloading",
                speed: item.speed > 0 ? item.speed : DEFAULT_SPEED,
                eta: (item.size - item.downloaded) / (item.speed || DEFAULT_SPEED),
              };
            }
            if (action === "pause") {
              return { ...item, status: "paused", speed: 0, eta: 0 };
            }
            return { ...item, status: "stopped", speed: 0, eta: 0 };
          }),
        };
      }),
    deleteCompleted: () =>
      setState((current) => ({
        ...current,
        downloads: current.downloads.filter((item) => !isFinished(item)),
        selected: current.selected.filter(
          (id) => !current.downloads.some((item) => item.id === id && isFinished(item)),
        ),
      })),
    addDownload: (input) =>
      setState((current) => {
        const nextId =
          current.downloads.reduce((max, item) => Math.max(max, item.id), 0) + 1;
        const now = Date.now();
        return {
          ...current,
          downloads: [
            ...current.downloads,
            {
              id: nextId,
              fileName: input.fileName,
              url: input.url,
              category: input.category,
              size: 0,
              downloaded: 0,
              status: "queued",
              speed: 0,
              eta: 0,
              addedAt: now,
              lastTryAt: now,
              savePath: input.savePath,
            },
          ],
        };
      }),
    pauseAll: () =>
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "downloading"
            ? { ...item, status: "paused", speed: 0, eta: 0 }
            : item,
        ),
      })),
    stopAll: () =>
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "downloading"
            ? { ...item, status: "stopped", speed: 0, eta: 0 }
            : item,
        ),
      })),
    redownloadSelected: () =>
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          current.selected.includes(item.id)
            ? { ...item, downloaded: 0, status: "queued", speed: 0, eta: 0 }
            : item,
        ),
      })),
    startQueue: () =>
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "queued"
            ? { ...item, status: "downloading", speed: DEFAULT_SPEED }
            : item,
        ),
      })),
    stopQueue: () =>
      setState((current) => ({
        ...current,
        downloads: current.downloads.map((item) =>
          item.status === "downloading"
            ? { ...item, status: "queued", speed: 0, eta: 0 }
            : item,
        ),
      })),
  };

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
        return (left.addedAt - right.addedAt) * factor;
    }
  });
}
