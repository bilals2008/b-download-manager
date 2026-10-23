export type DownloadStatus =
  | "queued"
  | "running"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

/** What one connection of a download is doing, as the engine reports it. */
export type SegmentStatus =
  | "pending"
  | "connecting"
  | "receiving"
  | "retrying"
  | "paused"
  | "stopped"
  | "done"
  | "failed";

/** One byte range of a download, fetched on its own connection. */
export interface DownloadSegment {
  id: number;
  start: number;
  /** The length of the range. Null when the server did not tell the size. */
  size: number | null;
  downloaded: number;
  status: SegmentStatus;
}

export interface DownloadInfo {
  id: number;
  url: string;
  dest: string;
  total: number;
  downloaded: number;
  connections: number;
  status: DownloadStatus;
  error: string | null;
  referer: string | null;
  /** The ranges of the download, with the state of each connection. */
  segments: DownloadSegment[];
  /** Whether the file can be resumed. Null until the server has answered. */
  resumable: boolean | null;
  /** The bandwidth cap of this download, in bytes per second. 0 is no cap. */
  speedLimit: number;
  /** The category picked in the add dialog, when one was picked. */
  category: string | null;
  /** The note typed in the add dialog, when one was typed. */
  description: string | null;
}

export const ACTIVE_STATUSES: DownloadStatus[] = ["queued", "running", "paused"];
