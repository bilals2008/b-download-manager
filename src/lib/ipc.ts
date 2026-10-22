import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
): Promise<number> {
  return invoke<number>("start_download", { url, dest, connections });
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

export function defaultDestination(): Promise<string> {
  return invoke<string>("default_destination");
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

/** Leave the application for good. */
export function quitApp(): Promise<void> {
  return invoke<void>("quit_app");
}
