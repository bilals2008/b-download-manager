import { useCallback, useEffect, useState } from "react";

import {
  adoptGrabberSnapshot,
  applyGrabberEvent,
  cancelGrabber,
  chunks,
  EMPTY_SNAPSHOT,
  grabberDownload,
  grabberError,
  grabberState,
  HANDOVER_CHUNK,
  onGrabberEvent,
  startGrabber,
  type GrabberFile,
  type GrabberItemResult,
  type GrabberOptions,
  type GrabberSnapshot,
} from "@/lib/grabber";

/** Where the selected files go, and how. */
export interface HandoverChoices {
  folder: string;
  connections: number;
  queue: "now" | "main";
}

export interface GrabberController {
  snapshot: GrabberSnapshot;
  results: GrabberItemResult[];
  error: string | null;
  /** `start` or `send` while that command is in flight. */
  busy: "start" | "send" | null;
  start: (options: GrabberOptions) => Promise<void>;
  cancel: () => Promise<void>;
  send: (files: GrabberFile[], choices: HandoverChoices) => Promise<void>;
  clearError: () => void;
}

/**
 * The bridge between the window and the crawl.
 *
 * The backend is the source of truth: this hook asks for its snapshot when the
 * window opens, then folds every event into that snapshot. A crawl that was
 * running before the window was closed is picked up where it is, because the
 * backend kept reading while no one was looking.
 */
export function useGrabber(): GrabberController {
  const [snapshot, setSnapshot] = useState<GrabberSnapshot>(EMPTY_SNAPSHOT);
  const [results, setResults] = useState<GrabberItemResult[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"start" | "send" | null>(null);

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;

    // Listen first, then ask: a file read while the answer is on its way
    // arrives as an event and is merged into the answer, not lost under it.
    void (async () => {
      try {
        const unlisten = await onGrabberEvent((event) => {
          setSnapshot((current) => applyGrabberEvent(current, event));
        });
        if (disposed) {
          unlisten();
          return;
        }
        stop = unlisten;
        const answer = await grabberState();
        if (!disposed) {
          setSnapshot((current) => adoptGrabberSnapshot(answer, current));
        }
      } catch {
        // In a plain browser preview there is no backend; the form still
        // renders and the start button reports the refusal.
      }
    })();

    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  const start = useCallback(async (options: GrabberOptions) => {
    setBusy("start");
    setError(null);
    setResults([]);
    try {
      await startGrabber(options);
      // The backend has the canonical snapshot, including whatever was read
      // between the call and its answer.
      const answer = await grabberState();
      setSnapshot((current) => adoptGrabberSnapshot(answer, current));
    } catch (failure) {
      setError(grabberError(failure));
    } finally {
      setBusy(null);
    }
  }, []);

  const cancel = useCallback(async () => {
    setError(null);
    try {
      await cancelGrabber();
      setSnapshot((current) =>
        current.status === "running"
          ? { ...current, status: "cancelling", message: "Stopping…" }
          : current,
      );
    } catch (failure) {
      setError(grabberError(failure));
    }
  }, []);

  const send = useCallback(
    async (files: GrabberFile[], choices: HandoverChoices) => {
      if (!files.length) {
        return;
      }
      setBusy("send");
      setError(null);
      const collected: GrabberItemResult[] = [];
      try {
        for (const group of chunks(files, HANDOVER_CHUNK)) {
          const answers = await grabberDownload({
            items: group.map((file) => ({
              url: file.url,
              name: file.name,
              category: file.category,
            })),
            folder: choices.folder,
            connections: choices.connections,
            queue: choices.queue === "main",
            referer: snapshot.url || undefined,
          });
          collected.push(...answers);
          setResults([...collected]);
        }
      } catch (failure) {
        setError(grabberError(failure));
      } finally {
        setResults([...collected]);
        setBusy(null);
      }
    },
    [snapshot.url],
  );

  const clearError = useCallback(() => setError(null), []);

  return { snapshot, results, error, busy, start, cancel, send, clearError };
}
