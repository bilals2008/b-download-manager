export type DownloadStatus =
  | "queued"
  | "running"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface DownloadInfo {
  id: number;
  url: string;
  dest: string;
  total: number;
  downloaded: number;
  connections: number;
  status: DownloadStatus;
  error: string | null;
}

export const ACTIVE_STATUSES: DownloadStatus[] = ["queued", "running", "paused"];
