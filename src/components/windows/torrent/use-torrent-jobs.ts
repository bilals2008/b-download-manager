import { useCallback, useEffect, useRef, useState } from "react";

import { describeError, isDesktop } from "@/lib/errors";
import { onTorrentJob, torrentJobs, type TorrentJob } from "@/lib/ipc-torrent";

/** How often the window asks the engine how the jobs are doing. */
const TICK = 1000;

interface TorrentJobsState {
  /** Every job of the session. */
  jobs: TorrentJob[];
  /** The job the reader is looking at. */
  chosen: string | null;
  /** The job the reader is looking at, when it is there. */
  job: TorrentJob | null;
  /** What went wrong, when something did. */
  problem: string | null;
  /** Look at another job. */
  select: (id: string) => void;
  /**
   * Do something to a job and read the jobs again. `patch` is drawn before the
   * command leaves, so a pause reads as pressed instead of as ignored.
   */
  run: (
    action: () => Promise<unknown>,
    patch?: (jobs: TorrentJob[]) => TorrentJob[],
  ) => Promise<void>;
  /** Whether something is being done right now. */
  busy: boolean;
}

/**
 * The jobs of the session, read once a second.
 *
 * The engine has no push of its own for a torrent, so the window asks. The
 * read is cheap and the picture it draws is a second old, which is the same
 * freshness the rest of the app shows.
 */
export function useTorrentJobs(): TorrentJobsState {
  const [jobs, setJobs] = useState<TorrentJob[]>([]);
  const [chosen, setChosen] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A read that is already on its way answers a job that is gone by then.
  const asked = useRef(0);

  const read = useCallback(async () => {
    if (!isDesktop()) {
      return;
    }
    const generation = ++asked.current;
    try {
      const list = await torrentJobs();
      if (generation !== asked.current) {
        return;
      }
      setJobs(list);
      setProblem(null);
    } catch (failure) {
      if (generation !== asked.current) {
        return;
      }
      setProblem(describeError(failure));
    }
  }, []);

  useEffect(() => {
    void read();
    // A hidden window has nothing to keep current; it reads once more when it
    // comes back, so it never draws a picture that is minutes old.
    const timer = window.setInterval(() => {
      if (!document.hidden) {
        void read();
      }
    }, TICK);
    const onVisible = () => {
      if (!document.hidden) {
        void read();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      asked.current += 1;
    };
  }, [read]);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let stopped = false;
    let unlisten: (() => void) | undefined;
    void onTorrentJob((id) => {
      setChosen(id);
      void read();
    })
      .then((stop) => {
        if (stopped) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch(() => {
        // The desktop runtime is not there; the first read still shows the jobs.
      });
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, [read]);

  // The first job is the one the reader looks at, until they pick another.
  const job = jobs.find((entry) => entry.infoHash === chosen) ?? null;
  useEffect(() => {
    if (job || jobs.length === 0) {
      return;
    }
    setChosen(jobs[0].infoHash);
  }, [job, jobs]);

  const run = useCallback(
    async (
      action: () => Promise<unknown>,
      patch?: (jobs: TorrentJob[]) => TorrentJob[],
    ) => {
      if (busy) {
        return;
      }
      setBusy(true);
      setProblem(null);
      if (patch) {
        setJobs((current) => patch(current));
      }
      try {
        await action();
      } catch (failure) {
        setProblem(describeError(failure));
      } finally {
        setBusy(false);
        await read();
      }
    },
    [busy, read],
  );

  return {
    jobs,
    chosen: job ? job.infoHash : null,
    job,
    problem,
    select: setChosen,
    run,
    busy,
  };
}
