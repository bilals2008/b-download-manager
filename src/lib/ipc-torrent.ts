/**
 * BitTorrent: magnet links and `.torrent` files.
 *
 * A job is not a download in the list: it holds its own metadata, its own
 * pieces and its own peers, and it is shown in the torrent window. The
 * files are written into a folder named after the torrent.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { invoke } from "@/lib/ipc";

/** What one file of the torrent is. */
export interface TorrentFile {
  /** The path of the file inside the torrent. */
  path: string;
  /** How many bytes it holds. */
  length: number;
  /** Where it starts in the stream of all the bytes. */
  offset: number;
  /** How much of the file is on disk, from 0 to 1. */
  progress: number;
  /** Whether the whole file is there. */
  done: boolean;
  /** Whether the reader kept this file. */
  selected: boolean;
}

/** One peer of the swarm, as the window shows it. */
export interface TorrentPeer {
  /** The peer's address. */
  address: string;
  /** The id the peer says. */
  peerId: string;
  /** The program the peer runs, when its id says so. */
  client: string;
  /** Whether the connection is there. */
  connected: boolean;
  /** How many bytes came from this peer. */
  downloaded: number;
  /** How many bytes went to this peer. */
  uploaded: number;
  /** How much of the torrent the peer has, from 0 to 1. */
  progress: number;
  /** How many bytes a second come from this peer. */
  downSpeed: number;
  /** How many bytes a second go to this peer. */
  upSpeed: number;
}

/** How a tracker is doing. */
export type TorrentTrackerState = "working" | "ok" | "error";

/** One tracker of the torrent. */
export interface TorrentTracker {
  /** The address of the tracker. */
  url: string;
  /** How the tracker is doing. */
  state: TorrentTrackerState;
  /** What the tracker said the last time, when it said something. */
  message: string | null;
  /** How many peers the tracker named. */
  peers: number;
  /** How many peers have the whole torrent. */
  seeders: number;
  /** How many peers have part of it. */
  leechers: number;
  /** How many seconds until the tracker is asked again. */
  nextIn: number | null;
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

/** Where the pieces of a torrent are. */
export interface TorrentPieces {
  /** How many bytes a piece holds. */
  pieceLength: number;
  /** How many pieces there are. */
  count: number;
  /** How many of them are on disk. */
  have: number;
  /** How many bytes the whole torrent holds. */
  totalLength: number;
}

/** One job, as the torrent window sees it. */
export interface TorrentJob {
  /** The info hash, the id of the job. */
  infoHash: string;
  /** The name of the torrent. */
  name: string;
  /** The folder the files are written into. */
  directory: string;
  /** What the magnet link said, when the job came from one. A magnet with
   * no display name carries `null`, not a missing field. */
  magnet: { infoHash: string; name: string | null; trackers: string[] } | null;
  /** The files of the torrent. */
  files: TorrentFile[];
  /** The files the reader chose. */
  selected: number[];
  /** The pieces of the torrent. */
  pieceMap: TorrentPieces;
  /** Which pieces are on disk, as a bitmap: one bit per piece, the first
   * piece in the highest bit of the first byte, in base64. A torrent of
   * twenty thousand pieces is two and a half kilobytes, not twenty thousand
   * booleans. */
  pieceBits: string;
  /** The piece that is coming in right now, when one is. */
  pieceInflight: number | null;
  /** The peers the trackers named. */
  peers: TorrentPeer[];
  /** The trackers of the torrent. */
  trackers: TorrentTracker[];
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
  /** How many bytes a second come in. */
  downSpeed: number;
  /** How many bytes a second go out. */
  upSpeed: number;
  /** How many seconds are left, when the speed says so. */
  eta: number | null;
  /** How many peers have the whole torrent. */
  seeders: number;
  /** How many peers have part of it. */
  leechers: number;
  /** When the job was added. */
  addedAt: number;
  /** What went wrong, when something did. */
  error: string | null;
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
  files: { path: string; length: number; offset: number }[];
  /** The trackers the torrent names. */
  trackers: string[];
}

/** The choices the reader makes about torrents. */
export interface TorrentSettings {
  /** Whether a finished torrent keeps uploading. */
  seeding: boolean;
  /** The upload limit in kilobytes a second; zero is no limit. */
  uploadLimitKbps: number;
  /** The ratio at which a finished torrent stops uploading; zero is never. */
  seedRatio: number;
  /** The minutes a finished torrent keeps uploading; zero is never. */
  seedMinutes: number;
  /** The port the trackers are told about. */
  port: number;
  /** Extra nodes the distributed table starts from, as `host:port` texts. */
  dhtNodes: string[];
  /** Whether torrents are announced on the local network (BEP 14). */
  lpd: boolean;
  /** Whether the router is asked to open the torrent port. */
  portMapping: boolean;
}

/** What the router's port mapping is doing. */
export type PortMappingStatus =
  | { state: "off" }
  | { state: "searching" }
  | { state: "mapped"; method: string; externalPort: number; leaseSecs: number }
  | { state: "noGateway" }
  | { state: "refused"; message: string };

/** Read what the router's port mapping is doing right now. */
export function torrentPortMapping(): Promise<PortMappingStatus> {
  return invoke<PortMappingStatus>("torrent_port_mapping");
}

/**
 * Read a torrent: its name and its files, before any byte is written. A
 * `.torrent` file answers at once; a magnet asks its peers for the list; an
 * address is fetched first and read like a file.
 */
export function torrentLookup(source: string): Promise<TorrentInfo> {
  return invoke<TorrentInfo>("torrent_lookup", { source });
}

/** The choices that change how a torrent is started. */
export interface TorrentStartOptions {
  /** Add the job paused, so it waits in the list. */
  startPaused?: boolean;
  /** Take pieces in file order instead of from whatever a peer has. */
  sequential?: boolean;
}

/**
 * Start a torrent: a magnet link, a `.torrent` file path or the address of a
 * `.torrent` file. `wanted` lists the files the reader kept; every file is
 * taken when it is left out.
 */
export function torrentStart(
  source: string,
  folder?: string,
  wanted?: readonly number[],
  options?: TorrentStartOptions,
): Promise<TorrentJob> {
  return invoke<TorrentJob>("torrent_start", {
    source,
    folder: folder ?? null,
    wanted: wanted ? [...wanted] : null,
    startPaused: options?.startPaused ?? null,
    sequential: options?.sequential ?? null,
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

/** Remove a job. The files go only when `deleteFiles` is true. */
export function torrentRemove(id: string, deleteFiles: boolean): Promise<void> {
  return invoke<void>("torrent_remove", { id, deleteFiles });
}

/** Check the files on disk against the torrent again. */
export function torrentRecheck(id: string): Promise<void> {
  return invoke<void>("torrent_recheck", { id });
}

/** Change the files a job takes. */
export function torrentSetFiles(
  id: string,
  wanted: readonly number[],
): Promise<void> {
  return invoke<void>("torrent_set_files", { id, wanted: [...wanted] });
}

/** Stop every job of the session where it is. */
export function torrentPauseAll(): Promise<void> {
  return invoke<void>("torrent_pause_all");
}

/** Let every job of the session go on. */
export function torrentResumeAll(): Promise<void> {
  return invoke<void>("torrent_resume_all");
}

/** The choices the reader makes about torrents. */
export function torrentSettings(): Promise<TorrentSettings> {
  return invoke<TorrentSettings>("torrent_settings");
}

/** Keep the choices the reader makes about torrents. */
export function setTorrentSettings(
  settings: TorrentSettings,
): Promise<TorrentSettings> {
  return invoke<TorrentSettings>("set_torrent_settings", { settings });
}

/** Open the torrent window. */
export function showTorrentWindow(): Promise<void> {
  return invoke<void>("show_torrent_window");
}

/**
 * The engine says a job started, so the window can show that job first.
 *
 * The window is opened by the engine itself, so it is already there when the
 * event comes; the event only says which job the reader just added.
 */
export function onTorrentJob(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<{ id: string }>("torrent://job", (event) =>
    handler(event.payload.id),
  );
}
