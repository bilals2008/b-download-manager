/**
 * Download domain model.
 *
 * The category names, the tree groups and the column set come out of the
 * string tables in `IDMan.exe` (see `ANALYSIS.md`): the categories are
 * General, Video, Music, Documents, Programs, Games, Compressed and Other, and
 * the categories pane is a tree with the groups "All Downloads", "Unfinished"
 * and "Finished" above them.
 */

export const CATEGORIES = [
  "General",
  "Video",
  "Music",
  "Documents",
  "Programs",
  "Games",
  "Compressed",
  "Other",
] as const;

export type CategoryName = (typeof CATEGORIES)[number];

export type DownloadStatus =
  | "downloading"
  | "queued"
  | "paused"
  | "stopped"
  | "completed"
  | "error";

export interface Download {
  id: number;
  fileName: string;
  url: string;
  category: CategoryName;
  /** Total size in bytes. */
  size: number;
  /** Bytes already on disk. */
  downloaded: number;
  status: DownloadStatus;
  /** Bytes per second while downloading. */
  speed: number;
  /** Seconds left, 0 when not downloading. */
  eta: number;
  /** Epoch millis the download was added. */
  addedAt: number;
  /** Epoch millis of the last connection attempt. */
  lastTryAt: number;
  savePath: string;
}

/** The categories pane filter. A category name filters to that category. */
export type DownloadFilter = "all" | "unfinished" | "finished" | CategoryName;

export const FILTER_ALL: DownloadFilter = "all";

export type SortKey =
  | "order"
  | "name"
  | "size"
  | "status"
  | "eta"
  | "speed"
  | "lastTry"
  | "path";

export interface SortState {
  key: SortKey;
  dir: "asc" | "desc";
}

export const STATUS_LABELS: Record<DownloadStatus, string> = {
  downloading: "Downloading",
  queued: "Queued",
  paused: "Paused",
  stopped: "Stopped",
  completed: "Completed",
  error: "Error",
};

export function isFinished(download: Download): boolean {
  return download.status === "completed";
}

export function isUnfinished(download: Download): boolean {
  return download.status !== "completed";
}

export function progressOf(download: Download): number {
  if (download.size <= 0) {
    return 0;
  }
  return Math.min(100, (download.downloaded / download.size) * 100);
}

/** "1.2 GB" style size, matching how IDM labels the Size column. */
export function formatSize(bytes: number): string {
  if (bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024)),
  );
  const value = bytes / 1024 ** exponent;
  const decimals = exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(decimals)} ${units[exponent]}`;
}

/** "128.4 KB/s" style transfer rate. */
export function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond <= 0) {
    return "";
  }
  return `${formatSize(bytesPerSecond)}/s`;
}

/** "1 min 12 s", "2 h 5 min" or an em dash when there is no estimate. */
export function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return "—";
  }
  if (seconds < 60) {
    return `${Math.round(seconds)} s`;
  }
  if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    return `${minutes} min ${Math.round(seconds % 60)} s`;
  }
  const hours = Math.floor(seconds / 3600);
  return `${hours} h ${Math.floor((seconds % 3600) / 60)} min`;
}

/** "21.10.2026 14:32" style timestamp for the date columns. */
export function formatDate(epochMillis: number): string {
  const date = new Date(epochMillis);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

const MINUTE = 60_000;

/**
 * Sample download list. The clone ships without a network engine, so the
 * workspace starts from a realistic set of entries and the store advances them
 * from there.
 */
export function seedDownloads(): Download[] {
  const now = Date.now();
  const seed: Array<
    Omit<Download, "id" | "addedAt" | "lastTryAt"> & {
      minutesAgo: number;
    }
  > = [
    {
      fileName: "ubuntu-24.04.1-desktop-amd64.iso",
      url: "https://releases.ubuntu.com/24.04.1/ubuntu-24.04.1-desktop-amd64.iso",
      category: "Compressed",
      size: 6_107_030_784,
      downloaded: 2_133_000_000,
      status: "downloading",
      speed: 4_194_304,
      eta: 947,
      savePath: "C:\\Downloads\\Ubuntu",
      minutesAgo: 12,
    },
    {
      fileName: "Blender-4.2.1-windows-x64.msi",
      url: "https://download.blender.org/release/Blender4.2/Blender-4.2.1-windows-x64.msi",
      category: "Programs",
      size: 335_544_320,
      downloaded: 335_544_320,
      status: "completed",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Installers",
      minutesAgo: 184,
    },
    {
      fileName: "Sintel.2010.1080p.BluRay.x264.mkv",
      url: "https://media.example.org/sintel/sintel-1080p.mkv",
      category: "Video",
      size: 1_288_490_188,
      downloaded: 1_288_490_188,
      status: "completed",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Movies",
      minutesAgo: 1_540,
    },
    {
      fileName: "Nevermind (Remastered).flac",
      url: "https://media.example.org/music/nevermind.flac",
      category: "Music",
      size: 341_492_761,
      downloaded: 128_974_848,
      status: "paused",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Music",
      minutesAgo: 62,
    },
    {
      fileName: "Annual-Report-2024.pdf",
      url: "https://example.org/reports/annual-report-2024.pdf",
      category: "Documents",
      size: 2_512_384,
      downloaded: 2_512_384,
      status: "completed",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Documents",
      minutesAgo: 320,
    },
    {
      fileName: "setup_x64_2026.2.exe",
      url: "https://cdn.example.org/app/setup-x64-2026.2.exe",
      category: "Programs",
      size: 188_743_680,
      downloaded: 0,
      status: "queued",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Installers",
      minutesAgo: 4,
    },
    {
      fileName: "City-Architectures-4K-Pack.zip",
      url: "https://cdn.example.org/assets/city-architectures-4k.zip",
      category: "Compressed",
      size: 933_890_048,
      downloaded: 122_931_072,
      status: "error",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Assets",
      minutesAgo: 45,
    },
    {
      fileName: "Series.S03E04.2160p.mkv",
      url: "https://media.example.org/series/s03e04.mkv",
      category: "Video",
      size: 1_503_238_554,
      downloaded: 452_984_832,
      status: "stopped",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Series",
      minutesAgo: 21,
    },
    {
      fileName: "Podcast-Episode-142.mp3",
      url: "https://media.example.org/podcasts/episode-142.mp3",
      category: "Music",
      size: 50_331_648,
      downloaded: 0,
      status: "queued",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Podcasts",
      minutesAgo: 2,
    },
    {
      fileName: "Design-Systems-Handbook.pdf",
      url: "https://example.org/books/design-systems-handbook.pdf",
      category: "Documents",
      size: 13_209_216,
      downloaded: 13_209_216,
      status: "completed",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Books",
      minutesAgo: 2_400,
    },
    {
      fileName: "Old-Game-Patch-v2.7.exe",
      url: "https://cdn.example.org/games/patch-2.7.exe",
      category: "Games",
      size: 671_088_640,
      downloaded: 671_088_640,
      status: "completed",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Games",
      minutesAgo: 5_200,
    },
    {
      fileName: "OpenTTD-13.2-installer.exe",
      url: "https://cdn.example.org/games/openttd-13.2-setup.exe",
      category: "Games",
      size: 57_671_680,
      downloaded: 0,
      status: "stopped",
      speed: 0,
      eta: 0,
      savePath: "C:\\Downloads\\Games",
      minutesAgo: 88,
    },
  ];

  return seed.map((entry, index) => {
    const addedAt = now - entry.minutesAgo * MINUTE;
    const { minutesAgo: _minutesAgo, ...rest } = entry;
    return {
      ...rest,
      id: index + 1,
      addedAt,
      lastTryAt: addedAt + 60_000,
    };
  });
}
