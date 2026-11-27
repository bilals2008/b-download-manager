import {
  useCallback,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";

import { cancelDownload, pauseDownload, resumeDownload } from "@/lib/ipc";
import { torrentPause, torrentResume } from "@/lib/ipc-torrent";
import {
  mediaCancel as cancelMedia,
  mediaResume as resumeMedia,
} from "@/lib/media";

import type { WorkspaceState } from "./types";

interface BulkDeps {
  /** The newest state, for the rows the command is about to act on. */
  latest: RefObject<WorkspaceState>;
  setState: Dispatch<SetStateAction<WorkspaceState>>;
  /** Show what a command that failed had to say. */
  run: (task: Promise<unknown>) => void;
}

/**
 * The three commands that act on everything at once.
 *
 * Each one draws its answer first and then tells the clients: the engine for
 * the downloads and the media jobs, the torrent client for the torrents, so a
 * window with all three kinds in it pauses all three.
 */
export function useBulkActions({ latest, setState, run }: BulkDeps) {
  const pauseAll = useCallback(() => {
    const items = latest.current.downloads.filter(
      (item) => item.status === "downloading" && !item.mediaJob,
    );
    if (items.length === 0) {
      return;
    }
    setState((current) => ({
      ...current,
      downloads: current.downloads.map((item) =>
        item.status === "downloading" && !item.mediaJob
          ? { ...item, status: "paused", speed: 0, eta: 0 }
          : item,
      ),
    }));
    run(
      Promise.all(
        items.map((item) =>
          item.torrent ? torrentPause(item.torrent) : pauseDownload(item.id),
        ),
      ),
    );
  }, [latest, run, setState]);

  const resumeAll = useCallback(() => {
    // The mirror of Pause All: everything that is holding. A media job has no
    // pause, but a stopped one continues from the bytes it kept.
    const items = latest.current.downloads.filter(
      (item) =>
        (item.mediaJob === undefined || item.mediaCanResume === true) &&
        (item.status === "paused" ||
          item.status === "stopped" ||
          item.status === "error"),
    );
    if (items.length === 0) {
      return;
    }
    setState((current) => ({
      ...current,
      downloads: current.downloads.map((item) =>
        item.status === "paused" ||
        item.status === "stopped" ||
        item.status === "error"
          ? { ...item, status: "downloading", speed: 0, eta: 0, error: null }
          : item,
      ),
    }));
    run(
      Promise.all(
        items.map((item) =>
          item.torrent
            ? torrentResume(item.torrent)
            : item.mediaJob
              ? resumeMedia(item.mediaJob)
              : resumeDownload(item.id),
        ),
      ),
    );
  }, [latest, run, setState]);

  const stopAll = useCallback(() => {
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
    run(
      Promise.all(
        items.map((item) =>
          item.torrent
            ? torrentPause(item.torrent)
            : item.mediaJob
              ? cancelMedia(item.mediaJob)
              : cancelDownload(item.id),
        ),
      ),
    );
  }, [latest, run, setState]);

  return { pauseAll, resumeAll, stopAll };
}
