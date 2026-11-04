import { Channel, invoke } from "@tauri-apps/api/core";

import { formatBytes } from "./utils.ts";

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

/** The step a running update is on, as the backend reports it. */
export type UpdateStage = "downloading" | "verifying" | "installing";

/** One real report from the running update. */
export interface UpdateProgress {
  /** The release actually being installed, which may be newer than the one
   * the window checked a moment ago. */
  version: string;
  stage: UpdateStage;
  /** Bytes received; the complete package size once it is verified. */
  downloaded: number;
  /** The size the server named, when it named one. */
  total: number | null;
}

/** What the window has to do once the installer has been started. */
export interface InstalledReport {
  /** False on Windows, where the installer restarts the application. */
  restartRequired: boolean;
}

/**
 * Download the verified update, report its real progress, and hand it to the
 * installer.
 *
 * Each report names the release the backend is actually installing, so a
 * release that moved between the check and the download is shown truthfully.
 * The plugin still drops a package whose signature does not match before it
 * touches the disk, and the command resolves only after the installer has
 * been started.
 */
export function installUpdate(
  onProgress: (progress: UpdateProgress) => void,
): Promise<InstalledReport> {
  const channel = new Channel<UpdateProgress>();
  channel.onmessage = onProgress;
  return invoke<InstalledReport>("install_update", {
    onProgress: channel,
  });
}

/** Restart into the version the installer placed (not Windows). */
export function restartApp(): Promise<void> {
  return invoke<void>("restart_app");
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

/** A line of release notes, with the markdown the release body is written in. */
export interface NoteLine {
  kind: "heading" | "bullet" | "text";
  text: string;
}

/**
 * Release notes as a reader should see them.
 *
 * A release body is markdown written for a forge's page, not for a small notes
 * box: the heading marks, the bullet dashes and the emphasis arrive as text
 * instead of being shown to the reader.
 */
export function releaseNoteLines(notes: string): NoteLine[] {
  const lines: NoteLine[] = [];
  for (const raw of notes.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || /^([-*_])\1{2,}$/.test(line)) continue;
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    const bullet = /^[-*+]\s+(.*)$/.exec(line);
    if (heading) {
      lines.push({ kind: "heading", text: plainLine(heading[1]) });
    } else if (bullet) {
      lines.push({ kind: "bullet", text: plainLine(bullet[1]) });
    } else {
      lines.push({ kind: "text", text: plainLine(line) });
    }
  }
  return lines;
}

/** One line of markdown reduced to its words: links keep the word, not the URL. */
function plainLine(line: string): string {
  return line
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/\*\*(.+?)\*\*|`([^`]+)`/g, (_, bold: string, code: string) => {
      return bold ?? code;
    })
    .trim();
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

/**
 * How far the download is, or null while the server has not named a total.
 *
 * A missing total is not zero percent: the meter runs an indeterminate
 * segment instead of pretending the download has not started.
 */
export function updatePercent(downloaded: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.max(0, Math.round((downloaded / total) * 100)));
}

/** The byte line under an update meter: received, and of what when known. */
export function updateSize(downloaded: number, total: number | null): string {
  const received = formatBytes(downloaded);
  return total ? `${received} of ${formatBytes(total)}` : `${received} downloaded`;
}

/** The one title the progress surface gives each real stage. */
export function updateStageLabel(stage: UpdateStage): string {
  switch (stage) {
    case "downloading":
      return "Downloading the update";
    case "verifying":
      return "Checking the download is genuine";
    case "installing":
      return "Installing and restarting";
  }
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
