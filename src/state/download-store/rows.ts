import { useCallback, useRef } from "react";

import {
  detectCategory,
  fileNameFromUrl,
  knownCategory,
  splitPath,
  type CategoryDefinition,
  type Download,
} from "@/lib/downloads";
import { etaSeconds, nextSpeedSample, type SpeedSample } from "@/lib/speed";
import type { DownloadInfo } from "@/types";

import { ENGINE_STATUS } from "./list";

/** The pieces of the store a row is built from. */
interface RowDeps {
  categoriesRef: { current: CategoryDefinition[] };
}

/**
 * Engine reports and media jobs turned into the rows the table draws, with the
 * speed and estimate the counters imply.
 */
export function useRowMapping({ categoriesRef }: RowDeps) {
  // The engine sends absolute counters, so speed is the delta between two
  // events. One sample per download, kept out of state to avoid a render per
  // packet. The exponential smoothing keeps the column from flickering.
  const samples = useRef(new Map<number, SpeedSample>());
  const sampleFor = useCallback(
    (info: Pick<DownloadInfo, "id" | "downloaded" | "total" | "status">) => {
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

  return { samples, sampleFor, toDownload };
}
