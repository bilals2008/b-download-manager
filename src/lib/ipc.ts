import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { DownloadInfo } from "@/types";

export function onDownloadUpdate(
  handler: (download: DownloadInfo) => void,
): Promise<UnlistenFn> {
  return listen<DownloadInfo>("download://update", (event) => handler(event.payload));
}

/** The close button is asking, because downloads are still active. */
export function onCloseRequested(
  handler: (active: number) => void,
): Promise<UnlistenFn> {
  return listen<{ active: number }>("app://close-requested", (event) =>
    handler(event.payload.active),
  );
}

/** A link the browser extension handed over. */
export interface BridgeDownload {
  url: string;
  name: string;
  referer: string;
}

/** A link that the browser asked the application to download. */
export function onBridgeDownload(
  handler: (payload: BridgeDownload) => void,
): Promise<UnlistenFn> {
  return listen<BridgeDownload>("bridge://download", (event) =>
    handler(event.payload),
  );
}

/** A link copied to the clipboard while the reader asked to watch it. */
export function onClipboardLink(
  handler: (url: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("clipboard://url", (event) => handler(event.payload));
}

/** What the File Exists dialog answered. */
export type ConflictChoice = "ask" | "rename" | "replace" | "skip";

export interface StartDownloadInput {
  url: string;
  dest: string;
  connections?: number;
  referer?: string;
  /** Add the row to the queue instead of starting it now. */
  queue?: boolean;
  /** The category the download belongs to. */
  category?: string;
  /** A note to keep with the download. */
  description?: string;
  /** The answer to a destination conflict. The default asks the window. */
  conflict?: ConflictChoice;
}

/** What the engine did with a start. A rename changes the destination. */
export interface StartOutcome {
  id: number | null;
  dest: string;
  skipped: boolean;
}

/** A start the File Exists dialog has to settle, or cannot perform. */
export interface StartRefusal {
  kind: "exists" | "active" | "busy" | "invalid" | "unsafe";
  message: string;
  path: string;
  suggested: string;
}

export function startDownload(input: StartDownloadInput): Promise<StartOutcome> {
  return invoke<StartOutcome>("start_download", {
    request: {
      url: input.url,
      dest: input.dest,
      connections: input.connections ?? null,
      referer: input.referer?.trim() || null,
      queue: input.queue ?? false,
      category: input.category?.trim() || null,
      description: input.description?.trim() || null,
      conflict: input.conflict ?? null,
    },
  });
}

export function listDownloads(): Promise<DownloadInfo[]> {
  return invoke<DownloadInfo[]>("list_downloads");
}

export function pauseDownload(id: number): Promise<void> {
  return invoke<void>("pause_download", { id });
}

export function resumeDownload(id: number): Promise<void> {
  return invoke<void>("resume_download", { id });
}

/**
 * Start a failed download again. `restart` forgets bytes the server cannot
 * continue; otherwise the engine resumes from the parts it still has.
 */
export function retryDownload(id: number, restart: boolean): Promise<void> {
  return invoke<void>("retry_download", { id, restart });
}

/**
 * Start a finished or failed download over from its first byte.
 *
 * The row is only replaced once `conflict` settles a taken destination, so a
 * cancelled File Exists dialog leaves the download as it was.
 */
export function redownloadDownload(
  id: number,
  conflict: ConflictChoice,
): Promise<StartOutcome> {
  return invoke<StartOutcome>("redownload_download", { id, conflict });
}

/** Move an idle download and its partial files to another folder. */
export function relocateDownload(id: number, folder: string): Promise<void> {
  return invoke<void>("relocate_download", { id, folder });
}

/**
 * Record the category and the note the properties window edited.
 *
 * Both are metadata: the file on disk and the resume state stay as they are.
 */
export function setDownloadMeta(
  id: number,
  category: string | null,
  description: string | null,
): Promise<void> {
  return invoke<void>("set_download_meta", {
    id,
    category: category?.trim() || null,
    description: description?.trim() || null,
  });
}

/**
 * Point a download at another file, taking what it already wrote along.
 *
 * `conflict` settles a file that is already at the new name; without one the
 * engine refuses with a `StartRefusal` and the window asks. Resolves with the
 * path the engine actually used, which a rename changes.
 */
export function setDownloadDestination(
  id: number,
  dest: string,
  conflict: ConflictChoice = "ask",
): Promise<string> {
  return invoke<string>("set_download_destination", { id, dest, conflict });
}

/** Park a download in the queue, keeping the bytes it already received. */
export function queueDownload(id: number): Promise<void> {
  return invoke<void>("queue_download", { id });
}

/** Take a download out of the queue and leave it paused. */
export function dequeueDownload(id: number): Promise<void> {
  return invoke<void>("dequeue_download", { id });
}

/** Delete the file of a finished download, then forget its row. */
export function deleteDownloadFile(id: number): Promise<void> {
  return invoke<void>("delete_download_file", { id });
}

export function cancelDownload(id: number): Promise<void> {
  return invoke<void>("cancel_download", { id });
}

export function removeDownload(id: number): Promise<void> {
  return invoke<void>("remove_download", { id });
}

/**
 * Put a download in a category. This changes the index only; the file and
 * its folder stay exactly where they are.
 */
export function setDownloadCategory(
  id: number,
  category: string | null,
): Promise<void> {
  return invoke<void>("set_download_category", { id, category });
}

/** Open the progress window of a download, or bring it back to the front. */
export function openProgressWindow(id: number): Promise<void> {
  return invoke<void>("open_progress_window", { id });
}

/**
 * Fold the connection details of the progress window away, or open them.
 *
 * The window is not resizable, so the backend sizes it from this; the height
 * travels there for anyone who has not asked for less motion.
 */
export function setProgressDetails(open: boolean): Promise<void> {
  return invoke<void>("set_progress_details", {
    open,
    animated: !window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  });
}

/**
 * Cap the bandwidth of one download. Zero means no cap.
 *
 * Resolves with the rate the engine now holds, which is the value it clamped
 * the request to.
 */
export function setDownloadSpeedLimit(
  id: number,
  bytesPerSecond: number,
): Promise<number> {
  return invoke<number>("set_download_speed_limit", { id, bytesPerSecond });
}

export function defaultDestination(): Promise<string> {
  return invoke<string>("default_destination");
}

/** What a server says about a link before the download starts. */
export interface LinkProbe {
  /** The name the server gives the file, when it gives one. */
  name: string | null;
  /** The size of the file. Null when the server did not tell it. */
  total: number | null;
  /** Whether the server honours a range request. */
  ranges: boolean;
  /** Whether a stopped download could continue from where it stopped. */
  resumable: boolean;
}

/** The name, the size and the resume support of the link at an address. */
export function probeLink(
  url: string,
  referer?: string,
): Promise<LinkProbe> {
  return invoke<LinkProbe>("probe_link", {
    url,
    referer: referer?.trim() || null,
  });
}

/** What the dialog needs to know about a destination before it starts. */
export interface DestinationInfo {
  /** The folder the file goes into. */
  folder: string;
  /** Whether the folder is there already. A missing one is created. */
  folderExists: boolean;
  /** Whether a file of that name is already there. */
  fileExists: boolean;
  /** Another download in the list writes to this exact file. */
  active: boolean;
  /** A free name beside it, for the Rename action. */
  suggestedName: string;
  suggestedPath: string;
}

/**
 * Check a destination before a download is created for it.
 *
 * Rejects with the reason the engine would refuse it, in words the dialog can
 * show beside the field.
 */
export function inspectDestination(dest: string): Promise<DestinationInfo> {
  return invoke<DestinationInfo>("inspect_destination", { dest });
}

/** The system folder picker. Null when the user cancels. */
export async function chooseFolder(defaultPath: string): Promise<string | null> {
  const picked = await open({
    directory: true,
    multiple: false,
    defaultPath: defaultPath.trim() || undefined,
  });
  return typeof picked === "string" ? picked : null;
}

/** Open a file or a folder with the program the system uses for it. */
export function openPath(path: string): Promise<void> {
  return invoke<void>("open_path", { path });
}

/** Cap the bandwidth of the whole application. Zero means no cap. */
export function setSpeedLimit(bytesPerSecond: number): Promise<number> {
  return invoke<number>("set_speed_limit", { bytesPerSecond });
}

/** The proxy new downloads go through. Empty means no proxy. */
export function setProxy(url: string): Promise<void> {
  return invoke<void>("set_proxy", { url: url.trim() || null });
}

/** How many connections the whole application may have open at once. */
export function setMaxConnections(max: number): Promise<number> {
  return invoke<number>("set_max_connections", { max });
}

/** The choices the tray and the close button follow, as saved on disk. */
export interface DesktopPreferences {
  minimizeToTray: boolean;
  closeToTray: boolean;
}

/** Whether the minimize button hides the window into the tray. */
export function setMinimizeToTray(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_minimize_to_tray", { on });
}

/** Whether closing the window keeps the application in the tray. */
export function setCloseToTray(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_close_to_tray", { on });
}

/** Read back what the backend saved, for a window that has no storage. */
export function desktopPreferences(): Promise<DesktopPreferences> {
  return invoke<DesktopPreferences>("desktop_preferences");
}

/** Hide the main window without changing the state of its downloads. */
export function hideMainWindow(): Promise<void> {
  return invoke<void>("hide_main_window");
}

/** Whether a finished download gets its own window. */
export function setShowCompleteDialog(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_show_complete_dialog", { on });
}

/**
 * The folder a download handed over by the browser is saved to.
 *
 * The extension starts its hand-over in the backend, which cannot read the
 * window's saved preferences, so the window pushes the folder here. An empty
 * folder means the system Downloads folder.
 */
export function setBrowserFolder(folder: string): Promise<string> {
  return invoke<string>("set_browser_folder", { folder });
}

/** Whether a new download opens its progress window. */
export function setShowProgressOnStart(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_show_progress_on_start", { on });
}

/** The connections a browser hand-over gets, since it has no dialog to ask. */
export function setBrowserConnections(count: number): Promise<number> {
  return invoke<number>("set_browser_connections", { count });
}

/** Whether the clipboard is watched for download links. */
export function setClipboardCapture(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_clipboard_capture", { on });
}

/**
 * Register or remove the operating system's startup entry.
 *
 * Resolves with the state the operating system reports, which is what the
 * switch follows rather than the request.
 */
export function setLaunchOnStartup(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_launch_on_startup", { on });
}

/** The capture rules the extension follows while the application runs. */
export interface BrowserPolicyInput {
  capture: boolean;
  ask: boolean;
  types: string[];
  skipSites: string[];
}

export function setBrowserPolicy(
  policy: BrowserPolicyInput,
): Promise<BrowserPolicyInput> {
  return invoke<BrowserPolicyInput>("set_browser_policy", { policy });
}

/** What the application can verify about the browser bridge. */
export interface BrowserStatus {
  bridgeRunning: boolean;
  port: number;
  api: number;
  /** The last authenticated extension request, in Unix milliseconds. */
  lastContactMs: number | null;
}

export function browserStatus(): Promise<BrowserStatus> {
  return invoke<BrowserStatus>("browser_status");
}

/** Whether a folder can hold a download, and how much room is left. */
export interface FolderStatus {
  path: string;
  exists: boolean;
  writable: boolean;
  freeBytes: number | null;
  error: string | null;
}

export function folderStatus(folder: string): Promise<FolderStatus> {
  return invoke<FolderStatus>("folder_status", { folder });
}

/** Check a proxy address before it is stored. Empty means no proxy. */
export function validateProxy(url: string): Promise<string> {
  return invoke<string>("validate_proxy", { url });
}

/** Where the log and the saved download index live. */
export interface AppPaths {
  dataDir: string;
  logPath: string;
}

export function appPaths(): Promise<AppPaths> {
  return invoke<AppPaths>("app_paths");
}

/** Leave the application for good. */
export function quitApp(): Promise<void> {
  return invoke<void>("quit_app");
}
