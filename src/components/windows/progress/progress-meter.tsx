import { cn } from "@/lib/utils";
import type { DownloadStatus } from "@/types";

interface ProgressMeterProps {
  /** From 0 to 100. */
  value: number;
  /** The server did not tell the size of the file. */
  unknown: boolean;
  status: DownloadStatus;
}

/** The fill carries the state, so a paused window never looks alive. */
const FILL: Record<DownloadStatus, string> = {
  queued: "bg-chrome-muted",
  running: "bg-progress",
  paused: "bg-chrome-muted",
  completed: "bg-success",
  failed: "bg-destructive",
  cancelled: "bg-chrome-muted",
};

/**
 * The meter of the download.
 *
 * A file whose size the server did not tell cannot have a percentage, so the
 * meter runs a short segment across its track instead of pretending to know
 * how far along it is. Reducing motion leaves a still segment.
 */
export function ProgressMeter({ value, unknown, status }: ProgressMeterProps) {
  const percent = Math.min(100, Math.max(0, value));
  const running = status === "running";

  return (
    <div
      role="progressbar"
      aria-label="Download progress"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={unknown ? undefined : Math.round(percent)}
      aria-valuetext={
        unknown
          ? running
            ? "Size unknown, receiving data"
            : "Size unknown"
          : `${percent.toFixed(2)}%`
      }
      className="relative h-2 w-full overflow-hidden rounded-full bg-chrome-line"
    >
      {unknown ? (
        running ? (
          <div
            aria-hidden="true"
            className={cn(
              "absolute inset-y-0 left-0 w-1/3 rounded-full motion-safe:animate-indeterminate motion-reduce:left-0 motion-reduce:w-1/2",
              FILL[status],
            )}
          />
        ) : null
      ) : (
        <div
          aria-hidden="true"
          className={cn(
            "h-full rounded-full transition-[width] duration-300 ease-out motion-reduce:transition-none",
            FILL[status],
          )}
          style={{ width: `${percent}%` }}
        />
      )}
    </div>
  );
}
