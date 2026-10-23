import { cn } from "@/lib/utils";
import type { Download } from "@/lib/downloads";

interface DownloadProgressProps {
  value: number;
  status: Download["status"];
  label: string;
  className?: string;
}

const FILL: Record<Download["status"], string> = {
  downloading: "bg-progress",
  completed: "bg-success",
  paused: "bg-chrome-muted",
  queued: "bg-chrome-muted",
  stopped: "bg-chrome-muted",
  error: "bg-destructive",
};

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
