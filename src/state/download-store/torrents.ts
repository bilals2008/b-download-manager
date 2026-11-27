import {
  useCallback,
  useEffect,
  useRef,
  type Dispatch,
  type SetStateAction,
} from "react";

import {
  categoryOfFile,
  knownCategory,
  type CategoryDefinition,
  type Download,
} from "@/lib/downloads";
import { isDesktop } from "@/lib/errors";
import { torrentSummaries, type TorrentSummary } from "@/lib/ipc-torrent";

import { forgetTorrentCategory, torrentCategoryOf } from "./torrent-categories";
import type { WorkspaceState } from "./types";

/**
 * Torrent rows live below the engine's ids and the media ids, so a torrent, a
 * media job and a plain download can share one list and never meet.
 */
const TORRENT_ROW_BASE = -3_000_000_000;

/** How often the list asks what the torrents are doing. */
const TICK = 1500;

type SetWorkspace = Dispatch<SetStateAction<WorkspaceState>>;

/** The status a torrent row is drawn with. */
function rowStatus(job: TorrentSummary): Download["status"] {
  switch (job.status.state) {
    case "paused":
      return "paused";
    case "completed":
    case "seeding":
      return "completed";
    case "failed":
      return "error";
    case "idle":
      return "queued";
    default:
      return "downloading";
  }
}

/**
 * The torrents of the session, shown in the download list beside the plain
 * downloads.
 *
 * A torrent is not an engine download, so it is read from its own small list:
 * the summaries carry a row's numbers and none of the peers, pieces or files
 * the torrent window draws. The rows are tagged with the info hash, which is
 * how the rest of the list knows to send a command to the torrent client
 * instead of the download engine.
 */
export function useTorrentRows(
  setState: SetWorkspace,
  categories: { current: readonly CategoryDefinition[] },
) {
  /** A torrent keeps its numeric id for the life of the list. */
  const assigned = useRef(new Map<string, number>());
  const nextSlot = useRef(0);
  const signature = useRef("");

  const toTorrentDownload = useCallback(
    (job: TorrentSummary, id: number, previous?: Download): Download => {
      const now = Date.now();
      const status = rowStatus(job);
      const done = status === "completed";
      return {
        id,
        torrent: job.infoHash,
        torrentFiles: job.files,
        fileName: job.name,
        url: job.directory,
        // The reader's choice wins over what the name suggests; a category
        // that was removed since is no choice at all, so the name decides.
        category:
          knownCategory(torrentCategoryOf(job.infoHash), categories.current) ??
          categoryOfFile(job.name, categories.current),
        size: job.totalLength,
        downloaded: done ? job.totalLength : job.downloaded,
        status,
        speed: status === "downloading" ? job.downSpeed : 0,
        eta: status === "downloading" ? (job.eta ?? 0) : 0,
        addedAt: previous?.addedAt ?? (job.addedAt || now),
        lastTryAt: status === "downloading" ? now : (previous?.lastTryAt ?? now),
        // A torrent writes a folder of files, so the row's folder is its own
        // directory and there is no single file to name.
        savePath: job.directory,
        dest: job.directory,
        referer: null,
        connections: 0,
        error: job.error,
        resumable: null,
        description: null,
      };
    },
    [categories],
  );

  const merge = useCallback(
    (current: WorkspaceState, jobs: TorrentSummary[]): WorkspaceState => {
      const known = new Map(current.downloads.map((item) => [item.id, item]));
      const live = new Set(jobs.map((job) => job.infoHash));
      for (const hash of [...assigned.current.keys()]) {
        if (!live.has(hash)) {
          assigned.current.delete(hash);
          forgetTorrentCategory(hash);
        }
      }
      const rows = jobs.map((job) => {
        let id = assigned.current.get(job.infoHash);
        if (id === undefined) {
          id = TORRENT_ROW_BASE - nextSlot.current;
          nextSlot.current += 1;
          assigned.current.set(job.infoHash, id);
        }
        return toTorrentDownload(job, id, known.get(id));
      });
      // A torrent that is gone is gone; the engine and media rows are untouched.
      const downloads = [
        ...current.downloads.filter((item) => item.torrent === undefined),
        ...rows,
      ];
      return {
        ...current,
        downloads,
        selected: current.selected.filter((id) =>
          downloads.some((item) => item.id === id),
        ),
      };
    },
    [toTorrentDownload],
  );

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let cancelled = false;
    let timer = 0;
    const tick = async () => {
      if (cancelled) {
        return;
      }
      // A window in the tray has no list to keep current.
      if (!document.hidden) {
        try {
          const jobs = await torrentSummaries();
          if (!cancelled) {
            const next = jobs
              .map(
                (job) =>
                  `${job.infoHash}:${job.status.state}:${job.downloaded}:${job.downSpeed}:${job.error ?? ""}:${job.name}`,
              )
              .join("|");
            if (next !== signature.current) {
              signature.current = next;
              setState((current) => merge(current, jobs));
            } else {
              // The engine rows reload beside this poll and replace the rows
              // they hold; put the torrent rows back instead of waiting for a
              // torrent to move.
              setState((current) =>
                jobs.some(
                  (job) =>
                    !current.downloads.some((item) => item.torrent === job.infoHash),
                )
                  ? merge(current, jobs)
                  : current,
              );
            }
          }
        } catch {
          // The list is a convenience; a poll that fails asks again.
        }
      }
      if (!cancelled) {
        timer = window.setTimeout(tick, TICK);
      }
    };
    void tick();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [merge, setState]);
}
