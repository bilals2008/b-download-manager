import {
  useCallback,
  useEffect,
  useRef,
  type Dispatch,
  type SetStateAction,
} from "react";

import {
  categoryOfFile,
  splitPath,
  type CategoryDefinition,
  type Download,
} from "@/lib/downloads";
import { isDesktop } from "@/lib/errors";
import { listMediaJobs, type MediaJobView } from "@/lib/media";
import { etaSeconds } from "@/lib/speed";
import type { DownloadInfo } from "@/types";

import { MEDIA_ROW_BASE } from "./list";
import type { WorkspaceState } from "./types";

/** Speed and estimate of one row, sampled from the engine counters. */
type SampleFor = (
  info: Pick<DownloadInfo, "id" | "downloaded" | "total" | "status">,
) => { speed: number; eta: number; at: number };

type SetWorkspace = Dispatch<SetStateAction<WorkspaceState>>;

/**
 * The media jobs of the video path, shown as rows above the engine ids.
 *
 * They are a coordinator on top of the engine, so they are read as their own
 * small list and shown in the same table.
 */
export function useMediaRows(
  setState: SetWorkspace,
  sampleFor: SampleFor,
  dismissed: { current: Set<string> },
  categories: { current: readonly CategoryDefinition[] },
) {
  /** When each media row first appeared, so it does not jump in the order. */
  const added = useRef(new Map<number, number>());
  const signature = useRef("");

  const toMediaDownload = useCallback(
    (job: MediaJobView, previous?: Download): Download => {
      const id = MEDIA_ROW_BASE + job.job;
      const now = Date.now();
      const addedAt = added.current.get(job.job) ?? now;
      added.current.set(job.job, addedAt);
      const sample = sampleFor({
        id,
        downloaded: job.downloaded,
        total: job.total,
        status: job.downloading ? "running" : "paused",
      });
      const { dir, base } = splitPath(job.dest ?? "");
      const fileName = base || job.title || `Media ${job.job}`;
      const status: Download["status"] = job.done
        ? "completed"
        : job.error
          ? "error"
          : job.downloading
            ? "downloading"
            : job.stage === "Stopped"
              ? "stopped"
              : "paused";
      return {
        id,
        mediaJob: job.job,
        fileName,
        url: job.pageUrl || job.title,
        // The finished name decides, so a sound saved as MP3 is Music, not
        // Video, and a custom rule claiming the type is respected.
        category: categoryOfFile(fileName, categories.current),
        size: job.total,
        downloaded: job.downloaded,
        status,
        speed: sample.speed,
        eta: etaSeconds(job.total, job.downloaded, sample.speed),
        addedAt,
        lastTryAt: job.downloading ? now : (previous?.lastTryAt ?? now),
        savePath: dir,
        dest: job.dest ?? "",
        referer: null,
        connections: 1,
        error: job.error,
        resumable: false,
        description: job.stage,
        mediaStage: job.stage,
        mediaSegmentsDone: job.segmentsDone,
        mediaSegmentsTotal: job.segmentsTotal,
        mediaProgress: job.fraction,
        mediaCanResume: job.canResume,
      };
    },
    [sampleFor, categories],
  );

  // Media jobs are a coordinator on top of the engine, so they are read as
  // their own small list and shown as rows in the same table.
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
      try {
        const jobs = await listMediaJobs();
        if (cancelled) {
          return;
        }
        const shown = jobs.filter((job) => {
          if (dismissed.current.has(job.pageUrl || job.title)) {
            return false;
          }
          // The failure sentence of a row is not a media title. A page whose
          // title is that sentence can otherwise hand the app a job that saves
          // the sentence as a file name.
          const names = `${job.title} ${job.pageUrl} ${job.dest ?? ""}`;
          if (names.includes("open the failure details")) {
            return false;
          }
          return (
            job.downloading ||
            job.done ||
            Boolean(job.error) ||
            job.stage === "Stopped"
          );
        });
        const next = shown
          .map(
            (job) =>
              `${job.job}:${job.state}:${job.downloaded}:${job.fraction}:${job.error ?? ""}:${job.stage ?? ""}`,
          )
          .join("|");
        const merge = (current: WorkspaceState): WorkspaceState => {
          const previous = new Map(
            current.downloads.map((item) => [item.id, item]),
          );
          const rows = shown.map((job) =>
            toMediaDownload(job, previous.get(MEDIA_ROW_BASE + job.job)),
          );
          // A media row that is gone is gone; the engine rows are untouched.
          const normal = current.downloads.filter((item) => !item.mediaJob);
          const downloads = [...normal, ...rows];
          return {
            ...current,
            downloads,
            selected: current.selected.filter((id) =>
              downloads.some((item) => item.id === id),
            ),
          };
        };
        if (next !== signature.current) {
          signature.current = next;
          setState(merge);
        } else {
          // The engine list reloads beside this poll and replaces the rows
          // it holds. When that lands after this poll, the media rows are
          // missing even though nothing about the jobs changed: put them
          // back instead of waiting for a job to move.
          setState((current) => {
            const missing = shown.some(
              (job) =>
                !current.downloads.some(
                  (item) => item.id === MEDIA_ROW_BASE + job.job,
                ),
            );
            return missing ? merge(current) : current;
          });
        }
      } catch {
        // The media job list is a convenience; a failed poll tries again.
      }
      if (!cancelled) {
        timer = window.setTimeout(tick, 900);
      }
    };
    void tick();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [setState, toMediaDownload]);

  return { mediaAdded: added };
}
