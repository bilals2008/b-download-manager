import { progressChunks } from "@/lib/progress";
import { cn } from "@/lib/utils";
import type { DownloadSegment, DownloadStatus, SegmentStatus } from "@/types";

interface ProgressMeterProps {
  /** From 0 to 100. */
  value: number;
  /** The server did not tell the size of the file. */
  unknown: boolean;
  status: DownloadStatus;
  /** The ranges of the download, drawn one block each, the way IDM does. */
  segments: DownloadSegment[];
  /** The size of the file, so each range can be placed along it. */
  total: number;
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

/** A block carries the state of its own connection: done, live, or stopped. */
const CHUNK_FILL: Record<SegmentStatus, string> = {
  pending: "bg-chrome-muted",
  connecting: "bg-progress",
  receiving: "bg-progress",
  retrying: "bg-progress",
  paused: "bg-chrome-muted",
  stopped: "bg-chrome-muted",
  done: "bg-success",
  failed: "bg-destructive",
};

/**
 * The meter of the download.
 *
 * Once the engine has planned the ranges, the meter is one block per
 * connection: the whole range sits dim on the track so the plan is visible,
 * and the part already on disk carries the state of that connection. A plan it
 * cannot place falls back to the single bar, and a file whose size the server
 * did not tell runs a short segment across the track instead of pretending to
 * know how far along it is.
 */
export function ProgressMeter({
  value,
  unknown,
  status,
  segments,
  total,
}: ProgressMeterProps) {
  const percent = Math.min(100, Math.max(0, value));
  const running = status === "running";
  const chunks = progressChunks(segments, total);

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
      {chunks ? (
        <div aria-hidden="true" className="absolute inset-0">
          {chunks.map((chunk) => (
            <div
              key={chunk.id}
              className="absolute inset-y-0 bg-chrome-muted/25 pr-px"
              style={{ left: `${chunk.left}%`, width: `${chunk.width}%` }}
            >
              <div
                className={cn(
                  "h-full transition-[width] duration-300 ease-out motion-reduce:transition-none",
                  CHUNK_FILL[chunk.status],
                )}
                style={{ width: `${chunk.fill}%` }}
              />
            </div>
          ))}
        </div>
      ) : unknown ? (
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
