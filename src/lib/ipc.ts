import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { DownloadInfo } from "@/types";

export function onDownloadUpdate(
  handler: (download: DownloadInfo) => void,
): Promise<UnlistenFn> {
  return listen<DownloadInfo>("download://update", (event) => handler(event.payload));
}

/** The commands the tray menu sends to the window. */
export function onTrayCommand(
  handler: (command: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("tray://command", (event) => handler(event.payload));
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

export function startDownload(
  url: string,
  dest: string,
  connections: number,
  referer?: string,
): Promise<number> {
  return invoke<number>("start_download", {
    url,
    dest,
    connections,
    referer: referer?.trim() || null,
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

export function cancelDownload(id: number): Promise<void> {
  return invoke<void>("cancel_download", { id });
}

export function removeDownload(id: number): Promise<void> {
  return invoke<void>("remove_download", { id });
}

/** Open the progress window of a download, or bring it back to the front. */
export function openProgressWindow(id: number): Promise<void> {
  return invoke<void>("open_progress_window", { id });
}

/**
 * Fold the connection details of the progress window away, or open them.
 *
 * The window is not resizable, so the backend sizes it from this.
 */
export function setProgressDetails(open: boolean): Promise<void> {
  return invoke<void>("set_progress_details", { open });
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
}

/** The name and the size the server gives the file at an address. */
export function probeLink(
  url: string,
  referer?: string,
): Promise<LinkProbe> {
  return invoke<LinkProbe>("probe_link", {
    url,
    referer: referer?.trim() || null,
  });
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

/** Whether closing the window hides it into the tray. */
export function setMinimizeToTray(on: boolean): Promise<boolean> {
  return invoke<boolean>("set_minimize_to_tray", { on });
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

/** Leave the application for good. */
export function quitApp(): Promise<void> {
  return invoke<void>("quit_app");
}
