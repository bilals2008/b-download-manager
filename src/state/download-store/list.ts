import {
  isFinished,
  isUnfinished,
  matchesStatusFilter,
  type CategoryName,
  type Download,
  type DownloadFilter,
  type SortState,
  type StatusFilter,
} from "@/lib/downloads";
import { folderLabel } from "@/lib/downloads";
import type { DownloadStatus as EngineStatus } from "@/types";

import type { CategoryFolder } from "./types";

/** Media rows live above the engine's ids, so the two can share one list. */
export const MEDIA_ROW_BASE = -1_000_000_000;

export const ENGINE_STATUS: Record<EngineStatus, Download["status"]> = {
  queued: "queued",
  running: "downloading",
  paused: "paused",
  completed: "completed",
  failed: "error",
  cancelled: "stopped",
};

/** What the Find bar looks at: the file name and the address it came from. */
export function matchesQuery(item: Download, query: string): boolean {
  return (
    item.fileName.toLowerCase().includes(query) ||
    item.url.toLowerCase().includes(query)
  );
}

/** The rows the query matches, in the order the list shows them. */
export function findMatchesFor(visible: Download[], query: string): number[] {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return [];
  }
  return visible
    .filter((item) => matchesQuery(item, needle))
    .map((item) => item.id);
}

/** Row count per categories pane entry. */
export function countByCategory(
  categories: { name: CategoryName }[],
  downloads: Download[],
): Record<string, number> {
  // A category may be called "toString"; a bare object would answer with a
  // function instead of a count.
  const result: Record<string, number> = Object.create(null);
  result.all = 0;
  result.unfinished = 0;
  result.finished = 0;
  for (const category of categories) {
    result[category.name] = 0;
  }
  for (const item of downloads) {
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
}

/** Save folders per category, for the tree children. */
export function foldersByCategory(
  categories: { name: CategoryName }[],
  downloads: Download[],
): Record<CategoryName, CategoryFolder[]> {
  const result = Object.create(null) as Record<CategoryName, CategoryFolder[]>;
  for (const category of categories) {
    result[category.name] = [];
  }
  const paths = new Map<CategoryName, Map<string, number>>();
  for (const item of downloads) {
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
}

export function filterDownloads(
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

export function sortDownloads(downloads: Download[], sort: SortState): Download[] {
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
