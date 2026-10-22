import { formatSize, formatSpeed } from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

/** One panel of the status bar. Neighbours are separated by space alone. */
const CELL =
  "flex shrink-0 items-center gap-1 px-3 tabular-nums";

/**
 * The status bar along the bottom of the window: how many downloads the
 * current category holds, how many are active or selected, and the total
 * transfer rate. Panels are separated by rules, the way a desktop status bar
 * is, and the size and the active count step aside on a narrow window so the
 * rate always has room.
 */
export function StatusBar() {
  const { visible, selected, downloads } = useDownloads();

  const totalSpeed = visible.reduce((sum, item) => sum + item.speed, 0);
  const totalSize = visible.reduce((sum, item) => sum + item.size, 0);
  const active = downloads.filter((item) => item.status === "downloading").length;

  return (
    <footer className="flex h-7 shrink-0 items-stretch overflow-hidden bg-chrome-canvas text-chrome-micro text-chrome-muted">
      <span className="flex min-w-0 items-center px-3 tabular-nums">
        <span className="truncate">
          {visible.length} of {downloads.length} downloads
        </span>
      </span>
      {active > 0 ? (
        <span className={`${CELL} hidden sm:flex`}>{active} active</span>
      ) : null}
      {selected.length > 0 ? (
        <span className={CELL}>{selected.length} selected</span>
      ) : null}
      <span className="ml-auto flex items-stretch">
        <span className={`${CELL} hidden sm:flex`}>
          {formatSize(totalSize)}
        </span>
        <span className={`${CELL} w-24 justify-end`}>
          {totalSpeed > 0 ? formatSpeed(totalSpeed) : "Idle"}
        </span>
      </span>
    </footer>
  );
}
