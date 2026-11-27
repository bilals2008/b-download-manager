import { useEffect, useRef } from "react";

import { playCompletionSound } from "@/lib/completion-sound";
import type { Download } from "@/lib/downloads";
import { loadSettings } from "@/lib/settings";

/** One sound per burst: ten files finishing together is one finished queue. */
const QUIET_MS = 1500;

/**
 * Play the completion sound once for each download that finishes.
 *
 * A row's previous status is what decides. A row that arrives already finished
 * — the list on a restart, or an engine id this window never saw running — is
 * not news and stays quiet, so the sound only ever answers something that
 * happened while the window was watching.
 *
 * The setting is read at the moment the sound plays, so a change in the
 * settings window is heard on the next download instead of the next start.
 */
export function useCompletionSound(downloads: readonly Download[]): void {
  const statuses = useRef(new Map<number, Download["status"]>());
  const quietUntil = useRef(0);

  useEffect(() => {
    const previous = statuses.current;
    const next = new Map<number, Download["status"]>();
    let finished = 0;

    for (const download of downloads) {
      next.set(download.id, download.status);
      const before = previous.get(download.id);
      if (
        download.status === "completed" &&
        before !== undefined &&
        before !== "completed"
      ) {
        finished += 1;
      }
    }

    statuses.current = next;
    if (finished === 0) {
      return;
    }

    const settings = loadSettings();
    if (!settings.completionSound) {
      return;
    }

    const now = Date.now();
    if (now < quietUntil.current) {
      return;
    }
    quietUntil.current = now + QUIET_MS;
    playCompletionSound(
      settings.completionSoundName,
      settings.completionSoundVolume,
    );
  }, [downloads]);
}
