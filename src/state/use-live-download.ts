import { useEffect, useRef, useState } from "react";

import { listen } from "@tauri-apps/api/event";

import { isDesktop } from "@/lib/errors";
import { listDownloads, onDownloadUpdate } from "@/lib/ipc";
import { etaSeconds, nextSpeedSample, type SpeedSample } from "@/lib/speed";
import type { DownloadInfo } from "@/types";

export interface LiveDownload {
  /** The newest state the engine reported for this download. */
  info: DownloadInfo;
  /** Smoothed bytes per second. */
  speed: number;
  /** Seconds left at that rate, or 0 when the size or rate is unknown. */
  eta: number;
}

/**
 * Follow one download from inside its progress window.
 *
 * The window opens with a copy of the state, so it has something to paint
 * before the first event. Every later state arrives on `download://update`,
 * the same broadcast the list reads; events for other downloads are dropped
 * here rather than re-rendering this window.
 *
 * The window is shared: it follows the download that was opened last, and the
 * backend says which one that is on `progress://job`. A download that arrives
 * there replaces the one on screen, and the rate starts again from nothing
 * because the bytes belong to another file.
 *
 * The rate is the difference between two reports, kept in a ref so sampling
 * does not itself cause a render. `nextSpeedSample` is shared with the list,
 * so the speed in the window and the speed in the row agree.
 */
export function useLiveDownload(initial: DownloadInfo): LiveDownload {
  const [id, setId] = useState(initial.id);
  const [info, setInfo] = useState(initial);
  const [rate, setRate] = useState({ speed: 0, eta: 0 });
  const sample = useRef<SpeedSample | undefined>(undefined);

  // The window follows the download that was opened last.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<{ job: number }>("progress://job", (event) => {
      if (disposed) {
        return;
      }
      // Another file: the rate of the old one says nothing about this one.
      sample.current = undefined;
      setId(event.payload.job);
    }).then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | undefined;
    let live = false;

    function apply(next: DownloadInfo) {
      const at = Date.now();
      const folded = nextSpeedSample(
        sample.current,
        next.downloaded,
        next.status === "running",
        at,
      );
      sample.current = folded;
      setInfo(next);
      setRate({
        speed: folded.speed,
        eta: etaSeconds(next.total, next.downloaded, folded.speed),
      });
    }

    void onDownloadUpdate((next) => {
      if (next.id !== id) {
        return;
      }
      live = true;
      apply(next);
    }).then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    });

    // An update can land between the copy the window was opened with and the
    // listener above. One read settles it; a later event always wins.
    void listDownloads()
      .then((all) => {
        if (disposed || live) {
          return;
        }
        const current = all.find((item) => item.id === id);
        if (current) {
          apply(current);
        }
      })
      .catch(() => {
        // The opening copy is still a state the engine reported; keep it.
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [id]);

  return { info, speed: rate.speed, eta: rate.eta };
}
