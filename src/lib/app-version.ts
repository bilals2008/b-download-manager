/**
 * The version the running build carries.
 *
 * The number comes from the application package, the same one the installer is
 * named after, so no screen has to keep a copy of it by hand. The read happens
 * once for the whole process and every window that asks is told when it lands.
 */
import { useSyncExternalStore } from "react";

import { isDesktop } from "@/lib/errors";

type Snapshot =
  | { status: "reading" }
  | { status: "ready"; version: string | null };

let snapshot: Snapshot = { status: "reading" };
let asked = false;
const listeners = new Set<() => void>();

function announce(): void {
  for (const listener of [...listeners]) {
    listener();
  }
}

/** Ask the application once; later callers get the answer already on the way. */
function ask(): void {
  if (asked) {
    return;
  }
  asked = true;
  if (!isDesktop()) {
    snapshot = { status: "ready", version: null };
    return;
  }
  void import("@tauri-apps/api/app")
    .then((app) => app.getVersion())
    .then(
      (version) => {
        snapshot = { status: "ready", version };
      },
      () => {
        snapshot = { status: "ready", version: null };
      },
    )
    .then(announce);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  ask();
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): Snapshot {
  return snapshot;
}

/**
 * The version of this build, or null while it is being read — which is also
 * what a window running in a browser gets, since there is no package there.
 */
export function useAppVersion(): string | null {
  const current = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return current.status === "ready" ? current.version : null;
}
