/**
 * Unpacking an archive that finished downloading.
 *
 * When the setting asks for it, a finished archive unpacks beside itself on
 * its own. The completion window shows the count of files while it happens,
 * asks for the password of an encrypted one, and says what came out — or why
 * it could not be.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { invoke } from "@/lib/ipc";

/** What the reader sees while the archive unpacks. */
export interface ExtractProgress {
  /** The download the archive belongs to. */
  id: number;
  /** How many files are in place so far. */
  files: number;
}

/** The archive that could not be unpacked because it needs a password. */
export interface ExtractPending {
  /** The download the archive belongs to. */
  id: number;
  /** The file the reader sees. */
  name: string;
}

/** The outcome of an unpack. */
export interface ExtractOutcome {
  /** The download the archive belongs to. */
  id: number;
  /** The folder the contents went into. */
  folder: string;
  /** How many files were put in place. */
  files: number;
  /** How many bytes were written. */
  bytes: number;
  /** How many entries were left out. */
  skipped: number;
  /** How many entries were refused. */
  refused: number;
  /** How many name collisions were renamed. */
  renamed: number;
}

/** Why an unpack could not be done. */
export interface ExtractError {
  /** The download the archive belongs to. */
  id: number;
  /** What went wrong, in words. */
  message: string;
}

/** What unpacking put in place. */
export interface ExtractReport {
  /** The folder the contents went into. */
  folder: string;
  /** How many files were put in place. */
  files: number;
  /** How many bytes were written. */
  bytes: number;
  /** How many entries were left out. */
  skipped: number;
  /** How many entries were refused. */
  refused: number;
  /** How many name collisions were renamed. */
  renamed: number;
}

/** The archive waiting for its password, when there is one. */
export function extractPending(): Promise<ExtractPending | null> {
  return invoke<ExtractPending | null>("extract_pending");
}

/** Unpack the archive with the password the reader typed. */
export function extractDownloadWithPassword(
  id: number,
  password: string,
): Promise<ExtractReport> {
  return invoke<ExtractReport>("extract_download_with_password", { id, password });
}

/** How many files are in place while an archive unpacks. */
export function onExtractProgress(
  handler: (event: ExtractProgress) => void,
): Promise<UnlistenFn> {
  return listen<ExtractProgress>("extract://progress", (event) =>
    handler(event.payload),
  );
}

/** The archive needs its password. */
export function onExtractNeedsPassword(
  handler: (event: ExtractPending) => void,
): Promise<UnlistenFn> {
  return listen<ExtractPending>("extract://needs-password", (event) =>
    handler(event.payload),
  );
}

/** What the unpack did. */
export function onExtractOutcome(
  handler: (event: ExtractOutcome) => void,
): Promise<UnlistenFn> {
  return listen<ExtractOutcome>("extract://outcome", (event) =>
    handler(event.payload),
  );
}

/** Why the unpack could not be done. */
export function onExtractFailed(
  handler: (event: ExtractError) => void,
): Promise<UnlistenFn> {
  return listen<ExtractError>("extract://failed", (event) =>
    handler(event.payload),
  );
}
