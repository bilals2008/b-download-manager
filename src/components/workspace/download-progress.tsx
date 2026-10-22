import { cn } from "@/lib/utils";
import type { Download } from "@/lib/downloads";

interface DownloadProgressProps {
  /** From 0 to 100. */
  value: number;
  status: Download["status"];
  label: string;
  className?: string;
}

/** The meter changes colour with the state, so a paused row never looks alive. */
const FILL: Record<Download["status"], string> = {
  downloading: "bg-primary",
  completed: "bg-success",
  paused: "bg-chrome-muted",
  queued: "bg-chrome-muted",
  stopped: "bg-chrome-muted",
  error: "bg-destructive",
};

/**
 * The progress of one row on a single line: a slim meter, then its percentage.
 * The bar carries the state in its colour, so a download that is paused or
 * failed reads as one without looking at the status column.
 */
export function DownloadProgress({
  value,
  status,
  label,
  className,
}: DownloadProgressProps) {
  const percent = Math.max(0, Math.min(100, value));

  return (
    <div className={cn("flex items-center justify-end gap-2", className)}>
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
        aria-valuetext={`${percent.toFixed(2)}%`}
        className="relative h-1.5 min-w-4 flex-1 overflow-hidden rounded-full bg-chrome-canvas"
      >
        <div
          className={cn(
            "h-full rounded-full transition-[width] duration-300 ease-out motion-reduce:transition-none",
            FILL[status],
          )}
          style={{ width: `${percent}%` }}
        />
      </div>
      <span className="w-12 shrink-0 text-right tabular-nums text-chrome-menu-text">
        {percent.toFixed(2)}%
      </span>
    </div>
  );
}
