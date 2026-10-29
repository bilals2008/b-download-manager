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
  detectCategory,
  fileNameFromUrl,
  folderLabel,
  isFinished,
  isUnfinished,
  joinPath,
  knownCategory,
  matchesStatusFilter,
  splitPath,
  type CategoryDefinition,
  type CategoryDraft,
  type CategoryName,
  type Download,
  type DownloadFilter,
  type SortKey,
  type SortState,
  type StatusFilter,
} from "@/lib/downloads";
import {
  OTHER_CATEGORY,
  categoryDraftProblem,
  loadCategories,
  saveCategories,
} from "@/lib/categories";
import {
  StartCancelled,
  startResolving,
  type BatchAsking,
  type ConflictAnswer,
  type ConflictReply,
  type FileConflict,
} from "@/lib/conflict";
import {
  cancelDownload,
  dequeueDownload,
  deleteDownloadFile,
  listDownloads,
  onBridgeDownload,
  onDownloadUpdate,
  pauseDownload,
  pushDownloadFolders,
  queueDownload,
  redownloadDownload,
  removeDownload,
  resumeDownload,
  setDownloadDestination,
  setDownloadMeta,
  startDownload,
  setDownloadCategory,
  type BridgeDownload,
  type ConflictChoice,
  type StartDownloadInput,
  type StartOutcome,
} from "@/lib/ipc";
import { describeError, isDesktop } from "@/lib/errors";
import { nextMatch } from "@/lib/find";
import { rangeIds } from "@/lib/selection";
import { loadSettings } from "@/lib/settings";
import { etaSeconds, nextSpeedSample, type SpeedSample } from "@/lib/speed";
import type { DownloadInfo, DownloadStatus as EngineStatus } from "@/types";

/** What a bulk action does to the selected rows. */
export type DownloadAction = "resume" | "pause" | "stop" | "delete";

/** How a click picks rows: plain, Ctrl or Cmd, Shift for a range, or both for a second range. */
export type SelectMode = "replace" | "toggle" | "range" | "add-range";

interface WorkspaceState {
  downloads: Download[];
  filter: DownloadFilter;
  /** The status filter of the list, inside the category filter. */
  statusFilter: StatusFilter;
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

/** A link the browser extension handed over, waiting in the add dialog. */
export interface BrowserRequest {
  url: string;
  name: string;
  /** The page the link was on, so the server accepts the request. */
  referer: string;
}

export interface DownloadStore {
  downloads: Download[];
  /** The category definitions, built in and user made. */
  categories: CategoryDefinition[];
  filter: DownloadFilter;
  /** The status filter applied inside the category filter. */
  statusFilter: StatusFilter;
  setStatusFilter: (filter: StatusFilter) => void;
  /** Add a user category. Returns what is wrong with it, or null. */
  addCategory: (draft: CategoryDraft) => string | null;
  /** Change a category. Returns what is wrong with it, or null. */
  updateCategory: (
    originalName: CategoryName,
    draft: CategoryDraft,
  ) => string | null;
  /**
   * Remove a category, moving what is left of its rows to Other. `deleteFiles`
   * deletes what the category holds first: finished files leave the disk and
   * unfinished rows stop and leave the list. Other cannot go.
   */
  removeCategory: (
    name: CategoryName,
    options?: { deleteFiles?: boolean },
  ) => Promise<string | null>;
  /** Put downloads in a category. The files themselves do not move. */
  assignCategory: (ids: number[], category: CategoryName) => void;
  /** Save the category and the note the properties window edited. */
  updateMeta: (
    id: number,
    category: CategoryName,
    description: string | null,
  ) => Promise<void>;
  /**
   * Move a download, settling a taken destination with `choice`. Resolves with
   * the path the engine used, or rejects with a `StartRefusal`.
   */
  changeDestination: (
    id: number,
    dest: string,
    choice: ConflictChoice,
  ) => Promise<string>;
  /** Delete the file of a finished row, then drop the row. */
  deleteFile: (id: number) => Promise<void>;
  /** Park rows in the queue, keeping what they already received. */
  queue: (ids: number[]) => void;
  /** Take rows out of the queue and leave them paused. */
  dequeue: (ids: number[]) => void;
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
  addDownload: (input: NewDownload) => Promise<AddOutcome>;
  /** A destination the list has to ask about, or null. */
  conflict: FileConflict | null;
  /** Settle the question the File Exists dialog is showing. */
  answerConflict: (answer: ConflictAnswer, applyToRemaining: boolean) => void;
  /** Dismiss the question; the start is abandoned. */
  cancelConflict: () => void;
  pauseAll: () => void;
  stopAll: () => void;
  redownloadSelected: () => void;
  deleteCompleted: () => void;
  startQueue: () => void;
  stopQueue: () => void;
}

/** What became of a start the window asked to begin. */
export type AddOutcome = "started" | "skipped" | "cancelled";

export interface NewDownload {
  url: string;
  fileName: string;
  category: CategoryName;
  savePath: string;
  /** The page the link was on, when the browser knew it. */
  referer?: string;
  /** Start the row in the queue instead of starting it now. */
  queue?: boolean;
  /** A note for the download properties. */
  description?: string;
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
  const [categories, setCategories] = useState<CategoryDefinition[]>(
    loadCategories,
  );
  // The engine events and the derived rows read the definitions at the moment
  // they run, so a rule added this second applies to the next row.
  const categoriesRef = useRef(categories);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [browserRequest, setBrowserRequest] = useState<BrowserRequest | null>(
    null,
  );
  // A destination that is taken, while the File Exists dialog asks about it.
  const [conflict, setConflict] = useState<FileConflict | null>(null);
  const conflictReply = useRef<((reply: ConflictReply | null) => void) | null>(
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
    const sample = nextSpeedSample(
      samples.current.get(info.id),
      info.downloaded,
      info.status === "running",
      now,
    );
    samples.current.set(info.id, sample);
    return {
      speed: sample.speed,
      eta: etaSeconds(info.total, info.downloaded, sample.speed),
      at: now,
    };
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
        // The engine keeps the category the dialog gave it, so a row looks
        // the same after a restart. An older index has none, or names a
        // category that has since been removed, and the file name is the
        // next best answer.
        category:
          knownCategory(info.category, categoriesRef.current) ??
          previous?.category ??
          detectCategory(fileName, categoriesRef.current),
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
        resumable: info.resumable,
        description: info.description,
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
          ? { ...next, addedAt: existing.addedAt }
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
    // A category may be called "toString"; a bare object would answer with a
    // function instead of a count.
    const result: Record<string, number> = Object.create(null);
    result.all = 0;
    result.unfinished = 0;
    result.finished = 0;
    for (const category of categories) {
      result[category.name] = 0;
    }
    for (const item of state.downloads) {
      result.all += 1;
      if (isFinished(item)) {
        result.finished += 1;
      }
      if (isUnfinished(item)) {
        result.unfinished += 1;
      }
      // A row left on a category that no longer exists is not in the tree.
      if (result[item.category] !== undefined) {
        result[item.category] += 1;
      }
    }
    return result;
  }, [categories, state.downloads]);

  const folders = useMemo(() => {
    const result = Object.create(null) as Record<CategoryName, CategoryFolder[]>;
    for (const category of categories) {
      result[category.name] = [];
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
  }, [categories, state.downloads]);

  /** Put the File Exists question on screen and wait for the answer. */
  function askConflict(question: FileConflict): Promise<ConflictReply | null> {
    return new Promise((resolve) => {
      // A question that is still waiting can only be replaced by the next one;
      // settle it as dismissed so its start does not hang on a promise nobody
      // will ever answer.
      conflictReply.current?.(null);
      conflictReply.current = resolve;
      setConflict(question);
    });
  }

  /** Take the answer out of the ref first, so a late click cannot land twice. */
  function settleConflict(reply: ConflictReply | null) {
    const settle = conflictReply.current;
    conflictReply.current = null;
    setConflict(null);
    settle?.(reply);
  }

  /** Start a finished or failed download over with a fresh engine job. */
  async function restart(
    item: Download,
    batch?: BatchAsking,
  ): Promise<AddOutcome> {
    let outcome: StartOutcome;
    try {
      // The engine replaces the row itself once the destination question is
      // settled, so a cancelled question leaves the row untouched.
      outcome = await startResolving(
        (choice) => redownloadDownload(item.id, choice),
        askConflict,
        batch,
      );
    } catch (failure) {
      if (failure instanceof StartCancelled) {
        return "cancelled";
      }
      throw failure;
    }
    if (outcome.id === null || outcome.skipped) {
      return "skipped";
    }
    // The row was replaced in the engine: the old id is gone, and the update
    // of the new one fills the list.
    forget(item.id);
    return "started";
  }

  /** Start one download and put its row in the list. */
  async function beginDownload(input: NewDownload): Promise<AddOutcome> {
    const dest = joinPath(input.savePath, input.fileName);
    // The options window owns both values, and they can change while the add
    // dialog is open, so they are read at the moment the download starts.
    const settings = loadSettings();
    const connections = settings.defaultConnections;
    const policy = settings.duplicatePolicy;
    const request: StartDownloadInput = {
      url: input.url,
      dest,
      connections,
      referer: input.referer,
      queue: input.queue,
      category: input.category,
      description: input.description,
    };
    let outcome: StartOutcome;
    try {
      outcome = await startResolving(
        (choice) => startDownload({ ...request, conflict: choice }),
        askConflict,
        // A chosen answer is the first attempt; a refusal that it cannot
        // settle still becomes the File Exists question.
        policy === "ask"
          ? undefined
          : { remaining: 0, choice: policy as ConflictAnswer },
      );
    } catch (failure) {
      if (failure instanceof StartCancelled) {
        return "cancelled";
      }
      throw failure;
    }
    if (outcome.id === null || outcome.skipped) {
      return "skipped";
    }
    const id = outcome.id;
    // A rename changes where the file goes, so the row follows the engine.
    const written = outcome.dest || dest;
    const now = Date.now();
    const { dir, base } = splitPath(written);
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
      dest: written,
      referer: input.referer?.trim() || null,
      connections,
      error: null,
      resumable: null,
      description: input.description?.trim() || null,
    };
    setState((current) => ({
      ...current,
      downloads: current.downloads.some((item) => item.id === id)
        ? current.downloads
        : [...current.downloads, row],
    }));
    return "started";
  }

  /**
   * A link the browser extension handed over.
   *
   * The browser waits while the user confirms it: the address and the name
   * the browser gave the file are put in the add dialog, which starts the
   * download when the user says so.
   */
  function takeFromBrowser(payload: BridgeDownload) {
    setBrowserRequest({
      url: payload.url,
      name: splitPath(payload.name).base,
      referer: payload.referer,
    });
  }

  /** Keep the definitions, the saved preference and the live rows in step. */
  function commitCategories(next: CategoryDefinition[]) {
    categoriesRef.current = next;
    setCategories(next);
    saveCategories(next);
    // The engine routes a new download to a category's folder, and it has no
    // preference file of its own to read the change from.
    if (isDesktop()) {
      void pushDownloadFolders(loadSettings().unfinishedFolder, next).catch(
        () => {
          // The engine keeps the folders it had; the next change or the next
          // start pushes them again.
        },
      );
    }
  }

  function addCategory(draft: CategoryDraft): string | null {
    const current = categoriesRef.current;
    const problem = categoryDraftProblem(draft, current);
    if (problem) {
      return problem;
    }
    commitCategories([
      ...current,
      {
        name: draft.name.trim(),
        extensions: [...draft.extensions],
        folder: draft.folder.trim(),
        builtin: false,
        icon: draft.icon,
      },
    ]);
    return null;
  }

  /**
   * Change a category. A rename reaches the index, so the rows keep the name
   * after a restart; a folder change stays a preference and never moves a file.
   */
  function updateCategory(
    originalName: CategoryName,
    draft: CategoryDraft,
  ): string | null {
    const current = categoriesRef.current;
    const original = current.find((category) => category.name === originalName);
    if (!original) {
      return "That category no longer exists.";
    }
    const problem = categoryDraftProblem(draft, current, originalName);
    if (problem) {
      return problem;
    }
    const name = draft.name.trim();
    commitCategories(
      current.map((category) =>
        category.name === originalName
          ? {
              ...category,
              name,
              folder: draft.folder.trim(),
              icon: draft.icon,
              // A built-in owns its file types; only its folder and icon are
              // editable.
              extensions: category.builtin
                ? category.extensions
                : [...draft.extensions],
            }
          : category,
      ),
    );
    if (name === originalName) {
      return null;
    }

    const affected = latest.current.downloads.filter(
      (item) => item.category === originalName,
    );
    setState((state) => ({
      ...state,
      filter: state.filter === originalName ? name : state.filter,
      downloads: state.downloads.map((item) =>
        item.category === originalName ? { ...item, category: name } : item,
      ),
    }));
    for (const item of affected) {
      run(setDownloadCategory(item.id, name));
    }
    return null;
  }

  /**
   * Remove a category. Its downloads move to Other and their files stay put,
   * unless the user asked for its contents to go too: a finished file is
   * deleted from the disk, an unfinished row stops and leaves the list with
   * its partial files in place. Other is the fallback every unmatched file
   * needs, so it cannot be removed.
   */
  async function removeCategory(
    name: CategoryName,
    options?: { deleteFiles?: boolean },
  ): Promise<string | null> {
    const current = categoriesRef.current;
    const target = current.find((category) => category.name === name);
    if (!target) {
      return "That category no longer exists.";
    }
    if (name === OTHER_CATEGORY) {
      return "Other holds every unmatched file, so it cannot be removed.";
    }

    const affected = latest.current.downloads.filter(
      (item) => item.category === name,
    );

    const deleted = new Set<number>();
    if (options?.deleteFiles) {
      for (const item of affected) {
        try {
          // A finished row owns a file; anything else owns parts, and the
          // engine keeps those on disk while it forgets the row.
          if (isFinished(item)) {
            await deleteDownloadFile(item.id);
          } else {
            await removeDownload(item.id);
          }
        } catch (failure) {
          return describeError(failure);
        }
        forget(item.id);
        deleted.add(item.id);
      }
    }

    commitCategories(current.filter((category) => category.name !== name));

    const remaining = affected.filter((item) => !deleted.has(item.id));
    setState((state) => ({
      ...state,
      filter: state.filter === name ? OTHER_CATEGORY : state.filter,
      filterPath: state.filter === name ? null : state.filterPath,
      downloads: state.downloads.map((item) =>
        item.category === name ? { ...item, category: OTHER_CATEGORY } : item,
      ),
    }));
    for (const item of remaining) {
      run(setDownloadCategory(item.id, OTHER_CATEGORY));
    }
    return null;
  }

  /** Put rows in a category. Only the index changes; no file is moved. */
  function assignCategory(ids: number[], category: CategoryName) {
    if (ids.length === 0) {
      return;
    }
    const changed = latest.current.downloads.filter(
      (item) => ids.includes(item.id) && item.category !== category,
    );
    if (changed.length === 0) {
      return;
    }
    setState((state) => ({
      ...state,
      downloads: state.downloads.map((item) =>
        ids.includes(item.id) ? { ...item, category } : item,
      ),
    }));
    for (const item of changed) {
      run(setDownloadCategory(item.id, category));
    }
  }

  /** Save the category and the note of one row. */
  async function updateMeta(
    id: number,
    category: CategoryName,
    description: string | null,
  ) {
    await setDownloadMeta(id, category, description);
  }

  /** Move one row. A taken destination rejects with the engine's refusal. */
  async function changeDestination(
    id: number,
    dest: string,
    choice: ConflictChoice,
  ): Promise<string> {
    return setDownloadDestination(id, dest, choice);
  }

  /** Delete the file of a finished row, then drop the row it belongs to. */
  async function deleteFile(id: number) {
    await deleteDownloadFile(id);
    forget(id);
  }

  /** Park rows in the queue. A finished row has nothing left to wait for. */
  function queue(ids: number[]) {
    const changed = latest.current.downloads.filter(
      (item) =>
        ids.includes(item.id) &&
        item.status !== "completed" &&
        item.status !== "queued",
    );
    if (changed.length === 0) {
      return;
    }
    setState((state) => ({
      ...state,
      downloads: state.downloads.map((item) =>
        changed.some((picked) => picked.id === item.id)
          ? { ...item, status: "queued", speed: 0, eta: 0 }
          : item,
      ),
    }));
    for (const item of changed) {
      run(queueDownload(item.id));
    }
  }

  /** Take queued rows out of the queue and leave them paused. */
  function dequeue(ids: number[]) {
    const changed = latest.current.downloads.filter(
      (item) => ids.includes(item.id) && item.status === "queued",
    );
    if (changed.length === 0) {
      return;
    }
    setState((state) => ({
      ...state,
      downloads: state.downloads.map((item) =>
        changed.some((picked) => picked.id === item.id)
          ? { ...item, status: "paused", speed: 0, eta: 0 }
          : item,
      ),
    }));
    for (const item of changed) {
      run(dequeueDownload(item.id));
    }
  }

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
    conflict,
    answerConflict: (answer, applyToRemaining) =>
      settleConflict({ choice: answer, applyToRemaining }),
    cancelConflict: () => settleConflict(null),
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
      // A live row has to be stopped before it can start over, so it is left
      // out instead of answering with an error nobody asked for.
      const items = latest.current.downloads.filter(
        (item) =>
          latest.current.selected.includes(item.id) &&
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
            outcome = await restart(items[index], batch);
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
  status: StatusFilter,
): Download[] {
  const scoped = (
    path === null
      ? downloads
      : downloads.filter((item) => item.savePath === path)
  ).filter((item) => matchesStatusFilter(item, status));
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
