/**
 * The Site Grabber: what the window asks for, what the crawl answers, and the
 * small pieces of arithmetic between the two.
 *
 * The crawl itself lives in Rust. This file is the contract: the shapes of the
 * events, the preferences the form remembers, and the pure helpers the window
 * uses to decide what is shown and what is sent. Keeping them here lets the
 * checks run them without a desktop window.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import { parseCategoryExtensions } from "./categories.ts";
import { describeError } from "./errors.ts";

/** The file groups the grabber offers. `other` is every unclaimed type. */
export type GrabberKind =
  | "video"
  | "music"
  | "documents"
  | "images"
  | "archives"
  | "programs"
  | "other";

export interface KindDefinition {
  id: GrabberKind;
  label: string;
  hint: string;
}

export const GRABBER_KINDS: readonly KindDefinition[] = [
  { id: "video", label: "Video", hint: "mp4, mkv, avi, mov, webm" },
  { id: "music", label: "Music", hint: "mp3, flac, wav, m4a, ogg" },
  { id: "documents", label: "Documents", hint: "pdf, docx, xlsx, epub" },
  { id: "images", label: "Images", hint: "jpg, png, gif, webp, svg" },
  { id: "archives", label: "Archives", hint: "zip, 7z, rar, iso" },
  { id: "programs", label: "Programs", hint: "exe, msi, dmg, apk" },
  { id: "other", label: "Other", hint: "types no group claims" },
];

export const KIND_LABELS: Record<GrabberKind, string> = {
  video: "Video",
  music: "Music",
  documents: "Documents",
  images: "Images",
  archives: "Archives",
  programs: "Programs",
  other: "Other",
};

/** The ticked groups when the window first opens. Pictures and unknown types
 *  turn a gallery or a page of assets into a file list, so they are opt-in. */
export const DEFAULT_KINDS: GrabberKind[] = [
  "video",
  "music",
  "documents",
  "archives",
  "programs",
];

export const MAX_DEPTH = 5;
export const MAX_PAGES = 500;
export const MAX_RULES = 32;
export const MAX_LOG = 300;
/** The window hands files over in chunks the backend accepts. */
export const HANDOVER_CHUNK = 25;

/** One file the crawl offered. */
export interface GrabberFile {
  url: string;
  name: string;
  extension: string;
  kind: GrabberKind;
  /** The built-in download category the file will be filed under. */
  category: string;
  /** The page it was found on. */
  page: string;
}

export type NoteLevel = "info" | "blocked" | "error";

/** One line of the crawl's log. */
export interface GrabberNote {
  level: NoteLevel;
  /** The address, without its query. */
  url: string;
  message: string;
}

export type GrabberStatus =
  | "idle"
  | "running"
  | "cancelling"
  | "done"
  | "cancelled";

/** Everything the window can learn about the current (or last) crawl. */
export interface GrabberSnapshot {
  status: GrabberStatus;
  url: string;
  site: string;
  depth: number;
  pageLimit: number;
  pagesRead: number;
  pagesQueued: number;
  files: GrabberFile[];
  notes: GrabberNote[];
  notesOmitted: number;
  duplicates: number;
  restricted: number;
  excluded: number;
  unsupported: number;
  truncated: boolean;
  message: string;
}

export const EMPTY_SNAPSHOT: GrabberSnapshot = {
  status: "idle",
  url: "",
  site: "",
  depth: 1,
  pageLimit: 0,
  pagesRead: 0,
  pagesQueued: 0,
  files: [],
  notes: [],
  notesOmitted: 0,
  duplicates: 0,
  restricted: 0,
  excluded: 0,
  unsupported: 0,
  truncated: false,
  message: "",
};

/** One step of a crawl, as the backend reports it. */
export type GrabberEvent =
  | {
      kind: "started";
      url: string;
      site: string;
      pages: number;
      depth: number;
    }
  | { kind: "page"; url: string; depth: number; links: number; files: number }
  | { kind: "file"; file: GrabberFile }
  | { kind: "note"; level: NoteLevel; url: string; message: string }
  | {
      kind: "progress";
      pagesRead: number;
      pagesQueued: number;
      files: number;
      pageLimit: number;
    }
  | {
      kind: "done";
      status: "done" | "cancelled" | "failed";
      message: string;
      pagesRead: number;
      pagesQueued: number;
      files: number;
      duplicates: number;
      restricted: number;
      excluded: number;
      unsupported: number;
      truncated: boolean;
    };

/** What the form sends to start reading. */
export interface GrabberOptions {
  url: string;
  depth: number;
  pages: number;
  types: GrabberKind[];
  extensions: string[];
  exclude: string[];
}

/** What one selected file becomes. */
export interface GrabberHandoverItem {
  url: string;
  name: string;
  category: string;
}

export interface GrabberHandover {
  items: GrabberHandoverItem[];
  folder: string;
  connections?: number;
  queue?: boolean;
  referer?: string;
}

export interface GrabberItemResult {
  url: string;
  name: string;
  status:
    | "started"
    | "queued"
    | "skipped"
    | "duplicate"
    | "invalid"
    | "refused";
  id: number | null;
  dest: string;
  message: string;
}

/* ---------------------------------------------------------------- commands */

export function openGrabberWindow(): Promise<void> {
  return invoke<void>("open_grabber_window");
}

export function startGrabber(options: GrabberOptions): Promise<void> {
  return invoke<void>("grabber_start", { request: options });
}

export function cancelGrabber(): Promise<void> {
  return invoke<void>("grabber_cancel");
}

export function grabberState(): Promise<GrabberSnapshot> {
  return invoke<GrabberSnapshot>("grabber_state");
}

export function grabberDownload(
  request: GrabberHandover,
): Promise<GrabberItemResult[]> {
  return invoke<GrabberItemResult[]>("grabber_download", { request });
}

export function onGrabberEvent(
  handler: (event: GrabberEvent) => void,
): Promise<UnlistenFn> {
  return listen<GrabberEvent>("grabber://event", (event) =>
    handler(event.payload),
  );
}

/* --------------------------------------------------------------- the state */

/**
 * Fold one event into the snapshot the window shows.
 *
 * The backend keeps the same snapshot, so this is not the source of truth: it
 * is what makes the list fill while the crawl runs. Reopening the window asks
 * the backend for the snapshot and starts folding again from there.
 */
export function applyGrabberEvent(
  snapshot: GrabberSnapshot,
  event: GrabberEvent,
): GrabberSnapshot {
  switch (event.kind) {
    case "started":
      return {
        ...snapshot,
        status: "running",
        url: event.url,
        site: event.site,
        depth: event.depth,
        pageLimit: event.pages,
        pagesRead: 0,
        pagesQueued: 0,
        files: [],
        notes: [],
        notesOmitted: 0,
        duplicates: 0,
        restricted: 0,
        excluded: 0,
        unsupported: 0,
        truncated: false,
        message: "",
      };
    case "page":
      return snapshot;
    case "file":
      if (snapshot.files.some((file) => file.url === event.file.url)) {
        return snapshot;
      }
      return { ...snapshot, files: [...snapshot.files, event.file] };
    case "note": {
      if (snapshot.notes.length >= MAX_LOG) {
        return { ...snapshot, notesOmitted: snapshot.notesOmitted + 1 };
      }
      return {
        ...snapshot,
        notes: [
          ...snapshot.notes,
          { level: event.level, url: event.url, message: event.message },
        ],
      };
    }
    case "progress":
      return {
        ...snapshot,
        pagesRead: event.pagesRead,
        pagesQueued: event.pagesQueued,
        pageLimit: event.pageLimit,
      };
    case "done":
      return {
        ...snapshot,
        status: event.status === "failed" ? "done" : event.status,
        message: event.message,
        pagesRead: event.pagesRead,
        pagesQueued: event.pagesQueued,
        duplicates: event.duplicates,
        restricted: event.restricted,
        excluded: event.excluded,
        unsupported: event.unsupported,
        truncated: event.truncated,
      };
    default:
      return snapshot;
  }
}

const STATUS_RANK: Record<GrabberStatus, number> = {
  idle: 0,
  running: 1,
  cancelling: 2,
  done: 3,
  cancelled: 3,
};

/**
 * Put the backend's snapshot under the events the window has already folded
 * in.
 *
 * The two travel separately: a file event can arrive while the snapshot that
 * would contain it is still on its way, and applying the older snapshot over
 * the newer event would hide that file until the next open. Files are merged
 * by address, and a crawl that has ended is not shown as running again by a
 * snapshot that was taken before it ended.
 */
export function adoptGrabberSnapshot(
  answer: GrabberSnapshot,
  current: GrabberSnapshot,
): GrabberSnapshot {
  const files = [...answer.files];
  const listed = new Set(files.map((file) => file.url));
  for (const file of current.files) {
    if (!listed.has(file.url)) {
      listed.add(file.url);
      files.push(file);
    }
  }
  return STATUS_RANK[current.status] > STATUS_RANK[answer.status]
    ? { ...current, files }
    : { ...answer, files };
}

/* ------------------------------------------------------------- the address */

/** The line under the address field, or null when it can be sent. */
export function grabberAddressProblem(value: string): string | null {
  const text = value.trim();
  if (!text) {
    return "Enter the address of the page to read.";
  }
  if (/\s/.test(text)) {
    return "The address cannot contain spaces.";
  }
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return "Enter a full address, like https://example.org/files/.";
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    return "Only http:// and https:// pages can be read.";
  }
  if (url.username || url.password) {
    return "An address with a user name or password is not followed.";
  }
  if (!url.hostname) {
    return "The address has no host.";
  }
  return null;
}

/** A path rule or pattern is a line: `*` matches anything in it. */
export function parseRules(text: string): string[] {
  const out: string[] = [];
  for (const token of text.split(/[\n,]+/)) {
    const rule = token.trim();
    if (!rule || rule.length > 200) {
      continue;
    }
    if (!out.some((known) => known.toLowerCase() === rule.toLowerCase())) {
      out.push(rule);
    }
    if (out.length >= MAX_RULES) {
      break;
    }
  }
  return out;
}

/** The extra extensions typed beside the groups. */
export function parseExtraExtensions(text: string): string[] {
  return parseCategoryExtensions(text);
}

/** The request the form describes. */
export function optionsFrom(
  url: string,
  prefs: GrabberPreferences,
): GrabberOptions {
  return {
    url: url.trim(),
    depth: clamp(prefs.depth, 1, MAX_DEPTH),
    pages: clamp(prefs.pages, 1, MAX_PAGES),
    types: prefs.types.length ? prefs.types : [...DEFAULT_KINDS],
    extensions: parseExtraExtensions(prefs.extensions),
    exclude: parseRules(prefs.exclude),
  };
}

/* ------------------------------------------------------------- preferences */

export interface GrabberPreferences {
  depth: number;
  pages: number;
  types: GrabberKind[];
  extensions: string;
  exclude: string;
  folder: string;
  connections: number;
  queue: "now" | "main";
}

export const DEFAULT_PREFERENCES: GrabberPreferences = {
  depth: 1,
  pages: 50,
  types: [...DEFAULT_KINDS],
  extensions: "",
  exclude: "",
  folder: "",
  connections: 8,
  queue: "now",
};

const PREFERENCES_KEY = "idm-clone.grabber.v1";

export function loadGrabberPreferences(): GrabberPreferences {
  try {
    const raw = window.localStorage.getItem(PREFERENCES_KEY);
    if (!raw) {
      return { ...DEFAULT_PREFERENCES, types: [...DEFAULT_PREFERENCES.types] };
    }
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return { ...DEFAULT_PREFERENCES, types: [...DEFAULT_PREFERENCES.types] };
    }
    const value = parsed as Partial<Record<keyof GrabberPreferences, unknown>>;
    const types = Array.isArray(value.types)
      ? GRABBER_KINDS.map((kind) => kind.id).filter((kind) =>
          (value.types as unknown[]).includes(kind),
        )
      : [];
    return {
      depth: clamp(number(value.depth, DEFAULT_PREFERENCES.depth), 1, MAX_DEPTH),
      pages: clamp(number(value.pages, DEFAULT_PREFERENCES.pages), 1, MAX_PAGES),
      types: types.length ? types : [...DEFAULT_KINDS],
      extensions: text(value.extensions, DEFAULT_PREFERENCES.extensions),
      exclude: text(value.exclude, DEFAULT_PREFERENCES.exclude),
      folder: text(value.folder, DEFAULT_PREFERENCES.folder),
      connections: clamp(
        number(value.connections, DEFAULT_PREFERENCES.connections),
        1,
        32,
      ),
      queue: value.queue === "main" ? "main" : "now",
    };
  } catch {
    return { ...DEFAULT_PREFERENCES, types: [...DEFAULT_PREFERENCES.types] };
  }
}

export function saveGrabberPreferences(prefs: GrabberPreferences): void {
  try {
    window.localStorage.setItem(PREFERENCES_KEY, JSON.stringify(prefs));
  } catch {
    // Storage can be blocked; the form still works for this session.
  }
}

/* ---------------------------------------------------------------- the list */

/** The files the filters show. */
export function visibleFiles(
  files: readonly GrabberFile[],
  kind: GrabberKind | "all",
  query: string,
): GrabberFile[] {
  const search = query.trim().toLowerCase();
  return files.filter((file) => {
    if (kind !== "all" && file.kind !== kind) {
      return false;
    }
    if (!search) {
      return true;
    }
    return (
      file.name.toLowerCase().includes(search) ||
      file.url.toLowerCase().includes(search)
    );
  });
}

/** The host and path of an address, without the query. */
export function filePath(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.host}${parsed.pathname}`;
  } catch {
    return url;
  }
}

/** An address reduced to one short line for a log row. */
export function shortAddress(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.hostname}${parsed.pathname}`;
  } catch {
    return url;
  }
}

/* -------------------------------------------------------------- the report */

export interface LeftOut {
  duplicates: number;
  blocked: number;
  excluded: number;
  unsupported: number;
  truncated: boolean;
}

export function leftOutOf(snapshot: GrabberSnapshot): LeftOut {
  return {
    duplicates: snapshot.duplicates,
    blocked: snapshot.restricted,
    excluded: snapshot.excluded,
    unsupported: snapshot.unsupported,
    truncated: snapshot.truncated,
  };
}

/** The one line under the list: what was found and what was left out. */
export function scanSummary(snapshot: GrabberSnapshot): string {
  const parts: string[] = [];
  parts.push(plural(snapshot.files.length, "file", "files"));
  if (snapshot.pagesRead) {
    parts.push(`${plural(snapshot.pagesRead, "page", "pages")} read`);
  }
  const left = leftOutOf(snapshot);
  if (left.blocked) parts.push(`${left.blocked} blocked`);
  if (left.excluded) parts.push(`${left.excluded} excluded`);
  if (left.duplicates) parts.push(`${left.duplicates} repeated`);
  if (left.unsupported) parts.push(`${left.unsupported} other types`);
  if (left.truncated) parts.push("limit reached");
  return parts.join(" · ");
}

/** The line in the status bar. */
export function progressLine(snapshot: GrabberSnapshot): string {
  switch (snapshot.status) {
    case "running":
      return snapshot.pagesRead === 0
        ? "Reading the first page…"
        : `Reading page ${Math.min(snapshot.pagesRead + 1, snapshot.pageLimit)} of ${snapshot.pageLimit}…`;
    case "cancelling":
      return "Stopping the crawl…";
    case "done":
    case "cancelled":
      return snapshot.message || scanSummary(snapshot);
    default:
      return "Enter an address, then read the site.";
  }
}

/** Whether the crawl is still going, in either direction. */
export function isRunning(snapshot: GrabberSnapshot): boolean {
  return snapshot.status === "running" || snapshot.status === "cancelling";
}

/** The selected files that have not been handed over yet, in list order. */
export function chosenFiles(
  files: readonly GrabberFile[],
  selected: ReadonlySet<string>,
  sent: ReadonlySet<string>,
): GrabberFile[] {
  return files.filter((file) => selected.has(file.url) && !sent.has(file.url));
}

/** The lines of the report under the list, one per answer. */
export function resultSummary(results: readonly GrabberItemResult[]): string {
  if (!results.length) {
    return "";
  }
  const count = (status: GrabberItemResult["status"]) =>
    results.filter((result) => result.status === status).length;
  const started = count("started");
  const queued = count("queued");
  const parts: string[] = [];
  if (started) parts.push(`${plural(started, "download", "downloads")} started`);
  if (queued) parts.push(`${queued} queued`);
  const duplicates = count("duplicate");
  if (duplicates) parts.push(`${duplicates} already in the list`);
  const invalid = count("invalid") + count("refused");
  if (invalid) parts.push(`${invalid} not started`);
  const skipped = count("skipped");
  if (skipped) parts.push(`${skipped} skipped`);
  return parts.join(" · ");
}

/** The answers that need a sentence: everything that did not start. */
export function notStarted(
  results: readonly GrabberItemResult[],
): GrabberItemResult[] {
  return results.filter(
    (result) =>
      result.status !== "started" &&
      result.status !== "queued" &&
      result.status !== "skipped",
  );
}

export function chunks<T>(items: readonly T[], size: number): T[][] {
  const out: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    out.push(items.slice(index, index + size));
  }
  return out;
}

/** Send a folder to the backend only when it is absolute. */
export function folderProblem(folder: string): string | null {
  const text = folder.trim();
  if (!text) {
    return "Choose a folder to save the files to.";
  }
  const absolute = /^[a-zA-Z]:[\\/]/.test(text) || text.startsWith("/") || text.startsWith("\\\\");
  return absolute ? null : "Choose a folder with the folder picker.";
}

/** Read the backend's refusal in the window's words. */
export function grabberError(error: unknown): string {
  return describeError(error);
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.round(value)));
}

function number(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function text(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}
