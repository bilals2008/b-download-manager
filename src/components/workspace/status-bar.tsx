import { formatSize, formatSpeed } from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

/**
 * The status bar along the bottom of the window: how many downloads the
 * current category holds, how many are active or selected, and the total
 * transfer rate. The size and the active count step aside on a narrow window so
 * the rate always has room.
 */
export function StatusBar() {
  const { visible, selected, downloads } = useDownloads();

  const totalSpeed = visible.reduce((sum, item) => sum + item.speed, 0);
  const totalSize = visible.reduce((sum, item) => sum + item.size, 0);
  const active = downloads.filter((item) => item.status === "downloading").length;

  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 overflow-hidden border-t border-chrome-line bg-chrome-menu px-3 text-xs text-chrome-muted">
      <span className="truncate tabular-nums">
        {visible.length} of {downloads.length} downloads
      </span>
      {active > 0 ? (
        <span className="hidden shrink-0 tabular-nums sm:inline">
          {active} active
        </span>
      ) : null}
      {selected.length > 0 ? (
        <span className="shrink-0 tabular-nums">{selected.length} selected</span>
      ) : null}
      <span className="ml-auto flex shrink-0 items-center gap-3 tabular-nums">
        <span className="hidden sm:inline">{formatSize(totalSize)}</span>
        <span className="w-20 text-right">
          {totalSpeed > 0 ? formatSpeed(totalSpeed) : "Idle"}
        </span>
      </span>
    </footer>
  );
}
