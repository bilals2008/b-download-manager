import { useCallback, useEffect, useRef, useState } from "react";

import { useAppVersionState } from "@/lib/app-version";
import { isDesktop } from "@/lib/errors";
import { dueReleaseFor, whatsNewFor } from "@/lib/whats-new-content";
import {
  hasUsedTheAppBefore,
  loadSeenVersion,
  saveSeenVersion,
  whatsNewStartupDecision,
  type WhatsNewRelease,
} from "@/lib/whats-new";

interface WhatsNewOptions {
  /** True while the first download list request is still in flight. */
  loading: boolean;
  /** Downloads that are transferring right now, media rows included. */
  activeDownloads: number;
  /** Another window or dialog already has the reader's attention. */
  blocked: boolean;
}

export interface WhatsNewController {
  open: boolean;
  release: WhatsNewRelease | null;
  /** Whether this open came from a menu or Settings rather than startup. */
  manual: boolean;
  /** True once the startup decision has been made, so other startup offers wait. */
  ready: boolean;
  /**
   * True for the whole launch when this version has unseen notes. The startup
   * tip and update check stand down instead of stacking on top of it.
   */
  freshNotes: boolean;
  openManually: () => void;
  dismiss: () => void;
}

/**
 * The automatic What's New popup.
 *
 * The version comes from the bundle and the seen version from storage. A new
 * version schedules its notes once and records the version when the popup
 * actually opens: a restart cannot repeat a version that was on screen, while
 * an offer deferred by a running download survives until the window is quiet.
 * A first install only records the starting point; it has no earlier version
 * to describe. A machine that already carries other saved choices is an
 * existing install from before this feature, so it gets the current release's
 * notes once.
 *
 * Opening waits for the first list request and for every transferring
 * download to finish. A popup never takes the window away from work that is
 * already running; the notes stay one click away in Settings > About.
 */
export function useWhatsNew({
  loading,
  activeDownloads,
  blocked,
}: WhatsNewOptions): WhatsNewController {
  const appVersion = useAppVersionState();
  const version = appVersion.status === "ready" ? appVersion.version : null;
  const [ready, setReady] = useState(false);
  const [freshNotes, setFreshNotes] = useState(false);
  const [pending, setPending] = useState<WhatsNewRelease | null>(null);
  const [open, setOpen] = useState(false);
  const [manual, setManual] = useState(false);
  const [release, setRelease] = useState<WhatsNewRelease | null>(null);
  const decided = useRef(false);

  // One decision per launch, after the version and the download list are
  // known. A new version stays pending until a quiet moment; it is recorded
  // when it actually opens, so a deferred offer survives a restart instead of
  // disappearing unseen.
  useEffect(() => {
    if (decided.current || appVersion.status !== "ready" || loading) {
      return;
    }
    decided.current = true;
    setReady(true);

    if (!isDesktop() || !version) {
      return;
    }

    const decision = whatsNewStartupDecision(
      version,
      loadSeenVersion(),
      hasUsedTheAppBefore(),
    );
    if (decision === "baseline") {
      saveSeenVersion(version);
      return;
    }
    if (decision !== "show") {
      return;
    }

    const notes = dueReleaseFor(version);
    if (!notes) {
      // A release shipped without a section. Record it so every launch does
      // not try again, and leave the newest section available by hand.
      saveSeenVersion(version);
      return;
    }

    setFreshNotes(true);
    setPending(notes);
  }, [appVersion.status, version, loading]);

  // Wait for a quiet moment: no other dialog and nothing transferring. The
  // record is written here, before the modal appears, so a restart while it
  // is on screen cannot show the same version twice.
  useEffect(() => {
    if (!pending || open || blocked || activeDownloads > 0) {
      return;
    }
    saveSeenVersion(pending.version);
    setRelease(pending);
    setPending(null);
    setManual(false);
    setOpen(true);
  }, [pending, open, blocked, activeDownloads]);

  const openManually = useCallback(() => {
    const notes = whatsNewFor(version);
    setPending(null);
    setRelease(notes);
    setManual(true);
    setOpen(true);
    // Opening the current version by hand is seeing it. A fallback to an
    // older section says nothing about this build, so it is left alone.
    if (version && notes?.version === version) {
      saveSeenVersion(version);
    }
  }, [version]);

  const dismiss = useCallback(() => {
    setPending(null);
    setOpen(false);
    setManual(false);
    setRelease(null);
  }, []);

  return {
    open,
    release,
    manual,
    ready,
    freshNotes,
    openManually,
    dismiss,
  };
}
