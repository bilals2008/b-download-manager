/**
 * What the progress window derives from the engine state.
 *
 * Everything here is a pure reading of one `DownloadInfo`: the percentage, the
 * words the window uses, and the conversions between the speed limit the user
 * types and the bytes per second the engine takes. The window components hold
 * the state; this module decides nothing that a test cannot check.
 */

import type { DownloadInfo, DownloadSegment, DownloadStatus, SegmentStatus } from "../types.ts";
import { formatSize } from "./downloads.ts";

export type SpeedUnit = "KB/s" | "MB/s";

export const SPEED_UNITS: readonly SpeedUnit[] = ["KB/s", "MB/s"];

/** 1 KB is 1024 bytes, the unit the Connection page already uses. */
export const KB = 1024;
export const MB = 1024 * 1024;

/** The largest cap either unit can express, one gigabyte a second. */
export const MAX_LIMIT_BYTES = 1024 * MB;

/** How far through a download is, from 0 to 100. */
export function percentOf(
  info: Pick<DownloadInfo, "status" | "total" | "downloaded">,
): number {
  // A finished download is all of itself: the engine drops the ranges and the
  // manifest, and a row restored from the index knows the size and not the
  // bytes.
  if (info.status === "completed") {
    return 100;
  }
  if (info.total <= 0) {
    return 0;
  }
  return Math.min(100, Math.max(0, (info.downloaded / info.total) * 100));
}

/** The percentage as the window prints it: 31.55% while it runs, 100% at the end. */
export function percentText(percent: number): string {
  const clamped = Math.min(100, Math.max(0, percent));
  return clamped >= 100 ? "100%" : `${clamped.toFixed(2)}%`;
}

/**
 * The percentage the window prints for a download.
 *
 * A file whose size the server did not tell cannot be a share of anything, so
 * it prints a dash instead of a percentage that would be a guess. A finished
 * file is 100% even though its size may be unknown.
 */
export function progressPercentText(
  info: Pick<DownloadInfo, "status" | "total" | "downloaded">,
): string {
  if (info.status === "completed") {
    return "100%";
  }
  return info.total > 0 ? percentText(percentOf(info)) : "—";
}

/** What the controls of the window can do for a download right now. */
export interface ProgressControls {
  /** The action the moment is waiting for. */
  primary: "pause" | "resume" | "open";
  canPause: boolean;
  canResume: boolean;
  canCancel: boolean;
  canOpen: boolean;
  canClose: boolean;
  /** The word on the primary button. */
  primaryLabel: string;
  /** What the primary button will do, for its tooltip. */
  primaryHint: string;
  /** What the cancel button will do, or why it cannot. */
  cancelHint: string;
}

/**
 * The controls of the status page, from the state of the download.
 *
 * A command that cannot act is not offered as an action: a finished download
 * has nothing to pause or cancel, and a stopped one has nothing left to stop.
 * A stopped download the server will not resume from can still be started
 * again, but the control says "Restart" because that is what it does. A paused
 * one keeps the word Resume: its transfer may still be open.
 */
export function progressControls(
  info: Pick<DownloadInfo, "status" | "resumable">,
): ProgressControls {
  if (info.status === "completed") {
    return {
      primary: "open",
      canPause: false,
      canResume: false,
      canCancel: false,
      canOpen: true,
      canClose: true,
      primaryLabel: "Open file",
      primaryHint: "Open the file that was downloaded.",
      cancelHint: "This download has already finished.",
    };
  }

  if (info.status === "running" || info.status === "queued") {
    return {
      primary: "pause",
      canPause: true,
      canResume: false,
      canCancel: true,
      canOpen: false,
      canClose: false,
      primaryLabel: "Pause",
      primaryHint:
        "Hold the connections back; the bytes that are already on disk stay.",
      cancelHint:
        "Stop this download and close this window. The bytes already received stay on disk.",
    };
  }

  // Paused, failed, or stopped. A paused download may still have its transfer
  // open, so it is always a resume: the engine continues the response rather
  // than asking the server for the range again. A stopped download of a file
  // the server will not resume from is a restart, and the button says so.
  const restart = info.resumable === false && info.status !== "paused";
  return {
    primary: "resume",
    canPause: false,
    canResume: true,
    canCancel: info.status !== "cancelled",
    canOpen: false,
    canClose: false,
    primaryLabel: restart ? "Restart" : "Resume",
    primaryHint: restart
      ? "Start the file again from the first byte; the server does not support resuming."
      : info.resumable === false
        ? "Continue the transfer that is still open. If it was stopped, the file starts again from the first byte."
        : "Continue from the bytes that are already on disk.",
    cancelHint:
      info.status === "cancelled"
        ? "This download is already stopped."
        : "Stop this download and close this window. The bytes already received stay on disk.",
  };
}

/** The status as a sentence: the engine's names, in the window's words. */
export function statusLabel(status: DownloadStatus): string {
  switch (status) {
    case "queued":
      return "Queued";
    case "running":
      return "Receiving data…";
    case "paused":
      return "Paused";
    case "completed":
      return "Complete";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Stopped";
  }
}

/** Whether a finished file can be picked up where it stopped. */
export function resumeLabel(resumable: boolean | null): string {
  if (resumable === null) {
    return "Not known yet";
  }
  return resumable ? "Yes" : "No";
}

/** What one connection is doing, in the words of the details table. */
export function connectionLabel(status: SegmentStatus): string {
  switch (status) {
    case "pending":
      return "Waiting";
    case "connecting":
      return "Connecting…";
    case "receiving":
      return "Receiving data…";
    case "retrying":
      return "Retrying…";
    case "paused":
      return "Paused";
    case "stopped":
      return "Stopped";
    case "done":
      return "Complete";
    case "failed":
      return "Failed";
  }
}

/**
 * The range a connection holds, as the details table prints it.
 *
 * A file whose size the server did not tell has one range that ends with the
 * response, so it has no length to show.
 */
export function segmentSize(segment: DownloadSegment): string {
  return segment.size === null ? "Unknown" : formatSize(segment.size);
}

/** "No limit" or "512 KB/s", for the row that states the cap in force. */
export function limitLabel(bytesPerSecond: number): string {
  if (bytesPerSecond <= 0) {
    return "No limit";
  }
  return `${formatSize(bytesPerSecond)}/s`;
}

/** The largest number the amount field takes in this unit. */
export function maxLimitAmount(unit: SpeedUnit): number {
  return unit === "MB/s" ? MAX_LIMIT_BYTES / MB : MAX_LIMIT_BYTES / KB;
}

/** The bytes per second a typed amount and unit mean. */
export function limitToBytes(amount: number, unit: SpeedUnit): number {
  const factor = unit === "MB/s" ? MB : KB;
  return Math.round(amount) * factor;
}

/**
 * The amount and unit a stored cap is shown in.
 *
 * A cap that is a whole number of megabytes is offered in megabytes; anything
 * else is shown in kilobytes, which is the unit a download limit is usually
 * typed in.
 */
export function bytesToLimit(bytesPerSecond: number): { amount: number; unit: SpeedUnit } {
  if (bytesPerSecond >= MB && bytesPerSecond % MB === 0) {
    return { amount: bytesPerSecond / MB, unit: "MB/s" };
  }
  if (bytesPerSecond >= KB && bytesPerSecond % KB === 0) {
    return { amount: bytesPerSecond / KB, unit: "KB/s" };
  }
  return {
    amount: Math.max(1, Math.round(bytesPerSecond / KB)),
    unit: "KB/s",
  };
}

/**
 * Read the amount field.
 *
 * `null` means the field does not hold a usable amount yet, so the Apply
 * button stays disabled instead of silently sending a guess.
 */
export function parseLimitAmount(text: string, unit: SpeedUnit): number | null {
  if (!/^\d+$/.test(text.trim())) {
    return null;
  }
  const amount = Number.parseInt(text, 10);
  if (!Number.isFinite(amount) || amount < 1 || amount > maxLimitAmount(unit)) {
    return null;
  }
  return amount;
}

/** How many connections are receiving data right now. */
export function receivingCount(segments: DownloadSegment[]): number {
  return segments.filter((segment) => segment.status === "receiving").length;
}

/**
 * The one-line summary over the connection table.
 *
 * "Receiving" is the most useful word while a download runs, but it is a poor
 * answer once everything is on disk or the user has stopped the download. The
 * summary names the state the connections are actually in.
 */
export function connectionSummary(segments: DownloadSegment[]): string {
  const total = segments.length;
  if (total === 0) {
    return "Waiting for the plan";
  }
  const count = (status: SegmentStatus) =>
    segments.filter((segment) => segment.status === status).length;

  if (count("done") === total) {
    return `${total} complete`;
  }
  const receiving = count("receiving");
  if (receiving > 0) {
    return `${receiving} receiving · ${total} total`;
  }
  const failed = count("failed");
  if (failed > 0) {
    return `${failed} failed · ${total} total`;
  }
  const stopped = count("stopped");
  if (stopped > 0) {
    return `${stopped} stopped · ${total} total`;
  }
  const paused = count("paused");
  if (paused > 0) {
    return `${paused} paused · ${total} total`;
  }
  const retrying = count("retrying");
  if (retrying > 0) {
    return `${retrying} retrying · ${total} total`;
  }
  const connecting = count("connecting");
  if (connecting > 0) {
    return `${connecting} connecting · ${total} total`;
  }
  return `${total} waiting`;
}
