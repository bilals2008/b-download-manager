import type {
  CategoryDefinition,
  CategoryDraft,
  CategoryName,
  Download,
  DownloadFilter,
  SortKey,
  SortState,
  StatusFilter,
} from "@/lib/downloads";
import type { ConflictAnswer, FileConflict } from "@/lib/conflict";
import type { ConflictChoice } from "@/lib/ipc";

/** What a bulk action does to the selected rows. */
export type DownloadAction = "resume" | "pause" | "stop" | "delete";

/** How a click picks rows: plain, Ctrl or Cmd, Shift for a range, or both for a second range. */
export type SelectMode = "replace" | "toggle" | "range" | "add-range";

export interface WorkspaceState {
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
  /** Start again every download that is paused, stopped or failed. */
  resumeAll: () => void;
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
  /** The export format the reader picked for a Drive editor link. */
  driveExport?: string | null;
}
