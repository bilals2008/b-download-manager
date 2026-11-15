/**
 * BitTorrent: magnet links and `.torrent` files.
 *
 * A job is not a download in the list: it holds its own metadata, its own
 * pieces and its own peers, and it is shown in the torrent window. The
 * files are written into a folder named after the torrent.
 */

import { invoke } from "@/lib/ipc";

/** What one file of the torrent is. */
export interface TorrentFile {
  /** The path of the file inside the torrent. */
  path: string;
  /** How many bytes it holds. */
  length: number;
  /** Where it starts in the stream of all the bytes. */
  offset: number;
}

/** One peer of the swarm, as the window shows it. */
export interface TorrentPeer {
  /** The peer's address. */
  address: string;
  /** The id the peer says. */
  peerId: string;
  /** Whether the connection is there. */
  connected: boolean;
  /** How many bytes came from this peer. */
  downloaded: number;
  /** How many bytes went to this peer. */
  uploaded: number;
  /** How much of the torrent the peer has, from 0 to 1. */
  progress: number;
}

/** What a job is doing. The `state` tag names it; `message` carries the
 * words for a failure. */
export type TorrentStatus =
  | { state: "idle" }
  | { state: "looking" }
  | { state: "noPeers" }
  | { state: "downloading" }
  | { state: "paused" }
  | { state: "completed" }
  | { state: "seeding" }
  | { state: "failed"; message: string };

/** One job, as the torrent window sees it. */
export interface TorrentJob {
  /** The info hash, the id of the job. */
  infoHash: string;
  /** The name of the torrent. */
  name: string;
  /** The folder the files are written into. */
  directory: string;
  /** What the magnet link said, when the job came from one. */
  magnet: { infoHash: string; name?: string; trackers: string[] } | null;
  /** The files of the torrent. */
  files: TorrentFile[];
  /** The files the reader chose. */
  selected: number[];
  /** The pieces of the torrent. */
  pieceMap: { pieceLength: number; count: number };
  /** The peers the trackers named. */
  peers: TorrentPeer[];
  /** What the job is doing. */
  status: TorrentStatus;
  /** How many bytes came in. */
  downloaded: number;
  /** How many bytes went out. */
  uploaded: number;
  /** How many bytes the torrent holds. */
  totalLength: number;
  /** Whether the job uploads after it is finished. */
  seeding: boolean;
}

/** What a torrent holds, before it is started. */
export interface TorrentInfo {
  /** The info hash, the id of the torrent. */
  infoHash: string;
  /** The name of the torrent. */
  name: string;
  /** How many bytes the torrent holds. */
  totalLength: number;
  /** The files of the torrent. */
  files: TorrentFile[];
}

/**
 * Read a torrent: its name and its files, before any byte is written. A
 * `.torrent` file answers at once; a magnet asks its peers for the list.
 */
export function torrentLookup(source: string): Promise<TorrentInfo> {
  return invoke<TorrentInfo>("torrent_lookup", { source });
}

/**
 * Start a torrent: a magnet link or a `.torrent` file path. `wanted` lists the
 * files the reader kept; every file is taken when it is left out.
 */
export function torrentStart(
  source: string,
  folder?: string,
  wanted?: readonly number[],
): Promise<TorrentJob> {
  return invoke<TorrentJob>("torrent_start", {
    source,
    folder: folder ?? null,
    wanted: wanted ? [...wanted] : null,
  });
}

/** Every job of the client. */
export function torrentJobs(): Promise<TorrentJob[]> {
  return invoke<TorrentJob[]>("torrent_jobs");
}

/** One job. */
export function torrentJob(id: string): Promise<TorrentJob> {
  return invoke<TorrentJob>("torrent_job", { id });
}

/** Stop a job where it is; the bytes that are there stay. */
export function torrentPause(id: string): Promise<void> {
  return invoke<void>("torrent_pause", { id });
}

/** Let a job go on. */
export function torrentResume(id: string): Promise<void> {
  return invoke<void>("torrent_resume", { id });
}

/** Turn the upload of a job on or off. */
export function torrentSetSeeding(id: string, seeding: boolean): Promise<void> {
  return invoke<void>("torrent_set_seeding", { id, seeding });
}

/** Whether a finished torrent keeps uploading. */
export function torrentSeeding(): Promise<boolean> {
  return invoke<boolean>("torrent_seeding");
}

/** Turn the upload of finished torrents on or off, and keep the choice. */
export function setTorrentSeeding(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_torrent_seeding", { enabled });
}

/** Remove a job. The files go only when `deleteFiles` is true. */
export function torrentRemove(id: string, deleteFiles: boolean): Promise<void> {
  return invoke<void>("torrent_remove", { id, deleteFiles });
}
