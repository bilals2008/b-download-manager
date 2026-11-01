import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { describeError } from "@/lib/errors";
import { mediaJob, type MediaJobView } from "@/lib/media";

/**
 * Follow one media job until its window closes.
 *
 * The backend owns the job; this only asks for it. Polling is cheap (the job
 * is a small object) and works for every state, including a download started
 * from another place. A newer job arrives as `video://job`, which is how the
 * one window serves every video without being rebuilt.
 */
export function useMediaJob(initialJob: number) {
  const [job, setJob] = useState(initialJob);
  const [view, setView] = useState<MediaJobView | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const fetchJob = useCallback(async (target: number) => {
    try {
      const next = await mediaJob(target);
      setView(next);
      setFailure(null);
    } catch (error) {
      setFailure(describeError(error));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timer = 0;
    const tick = async () => {
      if (cancelled) {
        return;
      }
      await fetchJob(job);
      if (!cancelled) {
        timer = window.setTimeout(tick, 850);
      }
    };
    void tick();

    let unlisten: UnlistenFn | undefined;
    void listen<{ job: number }>("video://job", (event) => {
      if (cancelled) {
        return;
      }
      setView(null);
      setFailure(null);
      setJob(event.payload.job);
    }).then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    });

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      unlisten?.();
    };
  }, [job, fetchJob]);

  const refresh = useCallback(() => fetchJob(job), [fetchJob, job]);
  return { job, view, failure, refresh };
}
