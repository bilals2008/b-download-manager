import {
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

import {
  OTHER_CATEGORY,
  categoryDraftProblem,
  loadCategories,
  saveCategories,
} from "@/lib/categories";
import {
  isFinished,
  type CategoryDefinition,
  type CategoryDraft,
  type CategoryName,
} from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import {
  dequeueDownload,
  queueDownload,
  deleteDownloadFile,
  pushCategoryRules,
  removeDownload,
  setDownloadCategory,
  setDownloadDestination,
  setDownloadMeta,
  type ConflictChoice,
} from "@/lib/ipc";
import { mediaForget } from "@/lib/media";

import {
  releaseTorrentCategory,
  rememberTorrentCategory,
  renameTorrentCategory,
} from "./torrent-categories";
import type { WorkspaceState } from "./types";

/** The pieces of the store the category work reaches for. */
interface CategoryDeps {
  latest: { current: WorkspaceState };
  setState: Dispatch<SetStateAction<WorkspaceState>>;
  run: (task: Promise<unknown>) => void;
  forget: (id: number) => void;
  dismissMedia: (url: string) => void;
}

/** The category tree, the files it holds and the rows that point at it. */
export function useCategories({
  latest,
  setState,
  run,
  forget,
  dismissMedia,
}: CategoryDeps) {
  const [categories, setCategories] = useState<CategoryDefinition[]>(
    loadCategories,
  );
  // The engine events and the derived rows read the definitions at the moment
  // they run, so a rule added this second applies to the next row.
  const categoriesRef = useRef(categories);

  /** Keep the definitions, the saved preference and the live rows in step. */
  function commitCategories(next: CategoryDefinition[]) {
    categoriesRef.current = next;
    setCategories(next);
    saveCategories(next);
    // The engine routes a new download to a category's folder, and it has no
    // preference file of its own to read the change from.
    if (isDesktop()) {
      void pushCategoryRules(next).catch(
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
    // A torrent row is rebuilt from the torrent summaries, so its category
    // lives in the torrent choices, not in the engine index.
    renameTorrentCategory(originalName, name);
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

    releaseTorrentCategory(name);
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
      // A torrent row is not an engine download: its category is kept beside
      // the torrent choices, so the next summary does not undo it.
      if (item.torrent === undefined) {
        run(setDownloadCategory(item.id, category));
      } else {
        rememberTorrentCategory(item.torrent, category);
      }
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
    const item = latest.current.downloads.find((row) => row.id === id);
    if (item?.mediaJob) {
      dismissMedia(item.url);
      await mediaForget(item.mediaJob, true);
      forget(id);
      return;
    }
    await deleteDownloadFile(id);
    forget(id);
  }

  /** Park rows in the queue. A finished row has nothing left to wait for. */
  function queue(ids: number[]) {
    const changed = latest.current.downloads.filter(
      (item) =>
        ids.includes(item.id) &&
        !item.mediaJob &&
        item.torrent === undefined &&
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
      (item) =>
        ids.includes(item.id) &&
        !item.mediaJob &&
        item.torrent === undefined &&
        item.status === "queued",
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


  return {
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
  };
}
