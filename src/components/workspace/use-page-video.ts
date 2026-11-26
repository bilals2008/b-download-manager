import { useEffect, useMemo, useRef, useState } from "react";

import { describeError } from "@/lib/errors";
import { pageMediaOpen } from "@/lib/ipc-page-media";
import {
  initialQuality,
  mediaDownload,
  mediaJob,
  mediaRows,
  type MediaJobView,
} from "@/lib/media";

interface PageVideoOptions {
  open: boolean;
  /** The address as it was pasted: the target of a redirect is not the page. */
  address: string;
  referer: string;
  /** Whether the page reads as one that plays a video. */
  videoPage: boolean;
  folder: string;
  /** The name the dialog would give the file, when the page gives none. */
  name: string;
  /** A start already in flight; the same guard the form's other actions use. */
  running: { current: boolean };
  setBusy: (busy: boolean) => void;
  setError: (message: string | null) => void;
  /** The download is going: save the folder and close the dialog. */
  onStarted: (folder: string) => void;
}

/**
 * The qualities of a page pasted into Add URL, drawn in the dialog itself.
 *
 * The panel's list is the same list: the page is resolved into a media job by
 * the backend, and the dialog asks that job for its rows. Nothing opens a
 * window, and the quality the reader picks starts the download the panel
 * would have started.
 */
export function usePageVideo({
  open,
  address,
  referer,
  videoPage,
  folder,
  name,
  running,
  setBusy,
  setError,
  onStarted,
}: PageVideoOptions) {
  const [job, setJob] = useState<number | null>(null);
  const [view, setView] = useState<MediaJobView | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const asked = useRef("");

  // A new address is a new page, and a reopened dialog asks again: the last
  // one's list and choice are not it.
  useEffect(() => {
    setJob(null);
    setView(null);
    setFailure(null);
    setPicked(null);
    asked.current = "";
  }, [address, open]);

  // The page is read once, as soon as it is known to play a video.
  useEffect(() => {
    if (!open || !videoPage || !address || asked.current === address) {
      return;
    }
    asked.current = address;
    let cancelled = false;
    pageMediaOpen(address, referer || undefined, undefined, true)
      .then((id) => {
        if (!cancelled) {
          setJob(id);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setFailure(describeError(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [open, address, referer, videoPage]);

  // Polling is cheap and works for every state: the job is a small object.
  useEffect(() => {
    if (job === null) {
      return;
    }
    let cancelled = false;
    let timer = 0;
    const tick = async () => {
      try {
        const next = await mediaJob(job);
        if (cancelled) {
          return;
        }
        setView(next);
        if (next.state === "parsing") {
          timer = window.setTimeout(tick, 850);
        }
      } catch (error) {
        if (!cancelled) {
          setFailure(describeError(error));
        }
      }
    };
    void tick();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [job]);

  const rows = useMemo(
    () => (view && view.state === "ready" ? mediaRows(view) : []),
    [view],
  );
  // A choice the reader made wins; a poll that replaces the rows must not
  // leave a quality behind that is no longer in them.
  const choice =
    picked && rows.some((row) => row.id === picked)
      ? picked
      : view && view.state === "ready"
        ? initialQuality(view)
        : null;
  const error = failure ?? (view?.state === "failed" ? view.error : null);

  async function download() {
    const row = rows.find((entry) => entry.id === choice);
    const target = folder.trim();
    if (running.current || !view || view.state !== "ready" || !row) {
      return;
    }
    if (!target) {
      setError("Choose a folder to save the video in.");
      return;
    }
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      await mediaDownload(view.job, {
        video: row.id,
        audio: row.audio,
        // The page's own title beats the last segment of its address.
        name: `${view.title || name} ${row.label}`.trim(),
        dest: target,
      });
      onStarted(target);
    } catch (reason) {
      setError(describeError(reason));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  return { view, rows, choice, error, choose: setPicked, download };
}
