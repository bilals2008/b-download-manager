import { formatSize, formatSpeed } from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

/**
 * The status bar along the bottom of the window: how many downloads the
 * current category holds, how many are selected, and the total transfer rate.
 */
export function StatusBar() {
  const { visible, selected, downloads } = useDownloads();

  const totalSpeed = visible.reduce((sum, item) => sum + item.speed, 0);
  const totalSize = visible.reduce((sum, item) => sum + item.size, 0);

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-chrome-line bg-chrome-menu px-3 text-xs text-chrome-muted">
      <span className="tabular-nums">
        {visible.length} of {downloads.length} downloads
      </span>
      {selected.length > 0 ? (
        <span className="tabular-nums">{selected.length} selected</span>
      ) : null}
      <span className="ml-auto flex items-center gap-4 tabular-nums">
        <span>{formatSize(totalSize)}</span>
        <span className="w-24 text-right">
          {totalSpeed > 0 ? formatSpeed(totalSpeed) : "Idle"}
        </span>
      </span>
    </footer>
  );
}
