import { invoke } from "@tauri-apps/api/core";

/**
 * The version check the "Check for updates" command performs.
 *
 * The Rust side owns the trusting: it asks the one release endpoint in the
 * application's own configuration and verifies every package against the
 * public key baked into the build. The window only shows the answer.
 */
export interface UpdateFound {
  available: boolean;
  currentVersion: string;
  version: string | null;
  /** The release date in RFC 3339 form, when the release carried one. */
  date: string | null;
  notes: string | null;
  error: string | null;
}

/** Ask the release endpoint for the newest version. */
export function checkUpdate(): Promise<UpdateFound> {
  return invoke<UpdateFound>("check_update");
}

/**
 * Download the verified update and hand the process to its installer.
 *
 * Refuses nothing here — the plugin drops a package whose signature does not
 * match before it touches the disk. Resolves only in failure; on success the
 * installer takes the application over.
 */
export function installUpdate(): Promise<void> {
  return invoke<void>("install_update");
}

/** When a release date reads as a day, not a timestamp. */
export function formatReleaseDate(date: string | null): string | null {
  if (!date) return null;
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return null;
  return parsed.toLocaleDateString(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

/**
 * The one answer a check gives, in the shape every screen that shows it uses.
 *
 * The update window and the About page both had to work this out from the
 * command's payload; keeping it here means a third screen cannot get it wrong.
 */
export type CheckOutcome =
  | { kind: "found"; update: UpdateFound }
  | { kind: "latest"; version: string }
  | { kind: "error"; message: string };

/** Put a reply into one of the three answers. */
export function outcomeOf(found: UpdateFound): CheckOutcome {
  if (found.error) {
    return { kind: "error", message: found.error };
  }
  if (found.available) {
    return { kind: "found", update: found };
  }
  return { kind: "latest", version: found.currentVersion };
}

/** A failure that arrived as anything at all, in one readable sentence. */
export function describeFailure(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Ask the release endpoint and answer in the form the screens show. */
export function runCheck(): Promise<CheckOutcome> {
  return checkUpdate().then(outcomeOf, (error: unknown): CheckOutcome => {
    return { kind: "error", message: describeFailure(error) };
  });
}

/** When the last check ran, or the four words that say it never did. */
export function formatCheckTime(millis: number | null): string {
  if (millis === null) return "Never checked.";
  const stamp = new Date(millis);
  if (Number.isNaN(stamp.getTime())) return "Never checked.";
  return `Checked ${stamp.toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  })}.`;
}
