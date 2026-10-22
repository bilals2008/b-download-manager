import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { DownloadInfo } from "@/types";

export function onDownloadUpdate(
  handler: (download: DownloadInfo) => void,
): Promise<UnlistenFn> {
  return listen<DownloadInfo>("download://update", (event) => handler(event.payload));
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
