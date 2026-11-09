/**
 * The media job the quality panel reads, and the pure helpers that turn its
 * formats into rows.
 *
 * The panel is a window of its own; the shapes here are the contract with
 * `src-tauri/src/video`. Nothing in this file touches a DOM: the window
 * components import it, and the local checks can run it without one.
 */

import { invoke } from "@tauri-apps/api/core";

export type MediaKind = "video" | "audio" | "muxed";
export type MediaState = "parsing" | "ready" | "failed";

export interface MediaFormat {
  id: string;
  kind: MediaKind;
  container: string;
  codec: string | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  bandwidth: number | null;
  size: number | null;
  label: string;
}

export interface MediaPair {
  video: string;
  audio: string | null;
  label: string;
}

export interface MediaJobView {
  job: number;
  state: MediaState;
  error: string | null;
  notice: string | null;
  pageUrl: string;
  title: string;
  duration: number | null;
  formats: MediaFormat[];
  pairs: MediaPair[];
  preferred: string | null;
  cookiesOffered: boolean;
  cookieConsent: boolean;
  downloading: boolean;
  done: boolean;
  dest: string | null;
  stage: string | null;
  downloaded: number;
  total: number;
  segmentsDone: number;
  segmentsTotal: number;
}

export type QualityKind = "video" | "audio";

/** One row of the quality list. */
export interface QualityRow {
  /** The video format id, or an audio format id for an audio-only row. */
  id: string;
  label: string;
  /** The bytes the manifest estimated, or null when it said nothing. */
  size: number | null;
  /** A sentence under the label, or null. */
  note: string | null;
  /** The sound track that goes with a video row, when the page offered one. */
  audio: string | null;
  kind: QualityKind;
  height: number | null;
  bandwidth: number | null;
}

/**
 * The rows the panel shows: one per video pairing, audio-only tracks last.
 *
 * A pair whose audio track the page never handed over stays a row with a note
 * that it carries no sound, instead of quietly saving a silent file.
 */
export function mediaRows(view: MediaJobView): QualityRow[] {
  const formats = new Map(view.formats.map((format) => [format.id, format]));
  const pairedAudio = new Set(
    view.pairs.map((pair) => pair.audio).filter((id): id is string => Boolean(id)),
  );

  const videoRows: QualityRow[] = [];
  for (const pair of view.pairs) {
    const video = formats.get(pair.video);
    if (!video || video.kind === "audio") {
      continue;
    }
    const audio = pair.audio ? formats.get(pair.audio) : undefined;
    let size: number | null = video.size;
    if (size !== null && audio) {
      size = audio.size === null ? size : size + audio.size;
    }
    const silent = video.kind === "video" && !pair.audio;
    videoRows.push({
      id: video.id,
      label: pair.label || video.label || video.id,
      size,
      note: silent ? "No sound" : null,
      audio: pair.audio,
      kind: "video",
      height: video.height,
      bandwidth: video.bandwidth,
    });
  }

  // A format the pairs do not mention (a manifest with no pair list, or an
  // extra video track) is still a row of its own.
  const listed = new Set(videoRows.map((row) => row.id));
  for (const format of view.formats) {
    if (format.kind === "audio" || listed.has(format.id)) {
      continue;
    }
    videoRows.push({
      id: format.id,
      label: format.label || format.id,
      size: format.size,
      note: format.kind === "video" ? "No sound" : null,
      audio: null,
      kind: "video",
      height: format.height,
      bandwidth: format.bandwidth,
    });
  }

  const audioRows: QualityRow[] = view.formats
    .filter((format) => format.kind === "audio" && !pairedAudio.has(format.id))
    .map((format) => ({
      id: format.id,
      label: format.label || format.id,
      size: format.size,
      note: null,
      audio: null,
      kind: "audio",
      height: null,
      bandwidth: format.bandwidth,
    }));

  videoRows.sort(byQuality);
  audioRows.sort(byQuality);
  return [...videoRows, ...audioRows];
}

function byQuality(left: QualityRow, right: QualityRow): number {
  const height = (right.height ?? -1) - (left.height ?? -1);
  if (height !== 0) {
    return height;
  }
  // At the same picture, the row that carries sound wins.
  const sound = Number(Boolean(left.note)) - Number(Boolean(right.note));
  if (sound !== 0) {
    return sound;
  }
  return (right.bandwidth ?? 0) - (left.bandwidth ?? 0);
}

/**
 * The row the panel opens on: this host's last choice, else 1080p, else the
 * closest lower quality that exists, else the smallest one above it.
 */
export function initialQuality(view: MediaJobView): string | null {
  const rows = mediaRows(view);
  if (!rows.length) {
    return null;
  }
  const preferred = (view.preferred || "").trim();
  if (preferred) {
    const exact = rows.find(
      (row) =>
        row.label.toLowerCase() === preferred.toLowerCase() ||
        row.label.toLowerCase().startsWith(`${preferred.toLowerCase()} `),
    );
    if (exact) {
      return exact.id;
    }
    const digits = Number.parseInt(preferred, 10);
    if (Number.isFinite(digits)) {
      return byHeight(rows, digits);
    }
  }
  return byHeight(rows, 1080);
}

function byHeight(rows: QualityRow[], target: number): string {
  const videos = rows.filter((row) => row.kind === "video" && row.height !== null);
  if (!videos.length) {
    return rows[0].id;
  }
  const sorted = [...videos].sort((left, right) => byQuality(left, right));
  const exact = sorted.find((row) => row.height === target);
  if (exact) {
    return exact.id;
  }
  const lower = sorted.find((row) => (row.height ?? 0) < target);
  if (lower) {
    return lower.id;
  }
  // Nothing is at or below the target: the smallest picture above it is the
  // closest one there is.
  return sorted[sorted.length - 1].id;
}

/** The quality word the panel remembers: the resolution the manifest stated. */
export function qualityToken(view: MediaJobView, id: string): string {
  const format = view.formats.find((entry) => entry.id === id);
  if (format?.height) {
    return `${format.height}p`;
  }
  return format?.label || "";
}

/** "1:02", "1:02:03"; the length the manifest stated, never an estimate. */
export function formatDuration(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) {
    return "";
  }
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(rest)}` : `${minutes}:${pad(rest)}`;
}

/* ---------------------------------------------------------------- commands */

export function mediaJob(job: number): Promise<MediaJobView> {
  return invoke<MediaJobView>("media_job", { job });
}

export interface MediaDownloadRequest {
  video: string;
  audio?: string | null;
  name?: string;
  dest?: string;
}

export function mediaDownload(
  job: number,
  request: MediaDownloadRequest,
): Promise<MediaJobView> {
  return invoke<MediaJobView>("media_download", { job, request });
}

/** Every media job the app knows, for the home list. */
export function listMediaJobs(): Promise<MediaJobView[]> {
  return invoke<MediaJobView[]>("media_jobs");
}

/** Forget a media row; `deleteFile` also removes the saved file. */
export function mediaForget(job: number, deleteFile: boolean): Promise<void> {
  return invoke<void>("media_forget", { job, deleteFile });
}

export function mediaCancel(job: number): Promise<void> {
  return invoke<void>("media_cancel", { job });
}

/** Allow the browser's cookies for media requests, once, and read again. */
export function mediaAllowCookies(job: number): Promise<MediaJobView> {
  return invoke<MediaJobView>("media_allow_cookies", { job });
}

/** Read the page again, for example after the video helper was installed. */
export function mediaReparse(job: number): Promise<MediaJobView> {
  return invoke<MediaJobView>("media_reparse", { job });
}

/** Whether the optional video helper is installed. */
export function videoHelperStatus(): Promise<boolean> {
  return invoke<boolean>("video_helper_status");
}

/** Download and check the optional video helper. */
export function videoHelperInstall(): Promise<boolean> {
  return invoke<boolean>("video_helper_install");
}

/** Keep a helper the person downloaded; `true` means it was checked. */
export function videoHelperImport(path: string): Promise<boolean> {
  return invoke<boolean>("video_helper_import", { path });
}

/** Forget the helper the person chose; go back to the app's own copy. */
export function videoHelperForget(): Promise<void> {
  return invoke<void>("video_helper_forget");
}

/** Whether the page is one the helper resolves better than the in-house path. */
export function isYoutubePage(url: string): boolean {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return (
      host === "youtu.be" ||
      host === "youtube.com" ||
      host.endsWith(".youtube.com") ||
      host.endsWith(".youtube-nocookie.com")
    );
  } catch {
    return false;
  }
}
