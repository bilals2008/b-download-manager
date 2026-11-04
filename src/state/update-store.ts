/**
 * The one running update of this window.
 *
 * The notice, the update dialog and the About page all read the same state, so
 * a download started from any of them keeps its real byte count when the
 * reader moves to another surface. The store owns no UI: it starts the command,
 * follows the channel it is given, and says which stage the backend is on.
 */
import { useSyncExternalStore } from "react";

import {
  describeFailure,
  installUpdate,
  restartApp,
  type UpdateFound,
  type UpdateProgress,
} from "@/lib/updates";

export type UpdatePhase =
  | "idle"
  | "downloading"
  | "verifying"
  | "installing"
  | "restart-ready"
  | "error";

export interface UpdateRun {
  phase: UpdatePhase;
  /** The release the reader agreed to install. */
  update: UpdateFound | null;
  downloaded: number;
  total: number | null;
  /** Bytes per second measured from the reports, while downloading. */
  speed: number | null;
  error: string | null;
}

const IDLE: UpdateRun = {
  phase: "idle",
  update: null,
  downloaded: 0,
  total: null,
  speed: null,
  error: null,
};

let snapshot: UpdateRun = IDLE;
let running: Promise<void> | null = null;
// The last second of real byte reports, used to read a stable speed.
let samples: { at: number; bytes: number }[] = [];
const listeners = new Set<() => void>();

function publish(next: UpdateRun): void {
  snapshot = next;
  for (const listener of [...listeners]) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): UpdateRun {
  return snapshot;
}

/**
 * The speed the reports themselves describe: the bytes gained over the last
 * second of samples, not a timer dressed as a download. A span too short to
 * mean anything keeps the previous reading.
 */
function measuredSpeed(bytes: number): number | null {
  const now = Date.now();
  samples.push({ at: now, bytes });
  while (samples.length > 2 && now - samples[0].at > 1000) {
    samples.shift();
  }
  if (samples.length < 2) {
    return null;
  }
  const first = samples[0];
  const last = samples[samples.length - 1];
  const seconds = (last.at - first.at) / 1000;
  if (seconds < 0.12) {
    return snapshot.speed;
  }
  const rate = (last.bytes - first.bytes) / seconds;
  return Number.isFinite(rate) && rate > 0 ? rate : null;
}

/** The running update, as every surface reads it. */
export function useUpdateRun(): UpdateRun {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** The running update, for an event handler that is not a component. */
export function currentUpdateRun(): UpdateRun {
  return snapshot;
}

/**
 * Start the update the reader agreed to.
 *
 * The stage only moves when the backend says so: `downloading` until the last
 * byte arrives, `verifying` while the plugin checks the signature, and
 * `installing` once the verified package is handed to its installer.
 */
export function beginUpdate(update: UpdateFound): void {
  if (running) {
    return;
  }
  samples = [];
  publish({
    phase: "downloading",
    update,
    downloaded: 0,
    total: null,
    speed: null,
    error: null,
  });

  // The backend names the release it is really installing. It is normally the
  // one on the card; if the release moved since the check, the card follows.
  let shown = update;
  const follow = (progress: UpdateProgress) => {
    shown = { ...shown, version: progress.version };
    const downloading = progress.stage === "downloading";
    if (!downloading) {
      samples = [];
    }
    publish({
      phase: progress.stage,
      update: shown,
      downloaded: progress.downloaded,
      total: progress.total ?? snapshot.total,
      speed: downloading ? measuredSpeed(progress.downloaded) : null,
      error: null,
    });
  };

  running = installUpdate(follow)
    .then(() => {
      publish({
        phase: "restart-ready",
        update: shown,
        downloaded: snapshot.downloaded,
        total: snapshot.total,
        speed: null,
        error: null,
      });
    })
    .catch((failure: unknown) => {
      publish({
        phase: "error",
        update: shown,
        downloaded: snapshot.downloaded,
        total: snapshot.total,
        speed: null,
        error: describeFailure(failure),
      });
    })
    .finally(() => {
      running = null;
    });
}

/** Try the same release again after a failure. */
export function retryUpdate(): void {
  const update = snapshot.update;
  if (update && snapshot.phase === "error") {
    publish(IDLE);
    beginUpdate(update);
  }
}

/** Leave the update surface after a failure or a finished install. */
export function clearUpdateRun(): void {
  if (running) {
    return;
  }
  samples = [];
  publish(IDLE);
}

/** Restart into the installed version, where the platform asks for it. */
export function restartApplication(): void {
  void restartApp().catch(() => {
    // The platform refused the restart; the installer's own restart still
    // runs on Windows, and the reader can close and reopen the application.
  });
}
