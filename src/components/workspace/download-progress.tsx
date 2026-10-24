import {
  gradientSpan,
  PROGRESS_FILL,
  type ProgressFillState,
} from "@/components/ui/progress-fill";
import { cn } from "@/lib/utils";
import type { Download } from "@/lib/downloads";

interface DownloadProgressProps {
  value: number;
  status: Download["status"];
  label: string;
  className?: string;
}

const FILL: Record<Download["status"], ProgressFillState> = {
  downloading: "active",
  completed: "complete",
  paused: "paused",
  queued: "queued",
  stopped: "stopped",
  error: "failed",
};

export function DownloadProgress({
  value,
  status,
  label,
  className,
}: DownloadProgressProps) {
  const percent = Math.max(0, Math.min(100, value));
  const span = gradientSpan(percent);

  return (
    <div className={cn("flex items-center justify-end gap-2", className)}>
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
        aria-valuetext={`${percent.toFixed(2)}%`}
        className="relative h-1.5 min-w-4 flex-1 overflow-hidden rounded-full bg-progress-track"
      >
        <div
          className={cn(
            "h-full rounded-full bg-no-repeat transition-[width] duration-300 ease-out motion-reduce:transition-none",
            PROGRESS_FILL[FILL[status]],
          )}
          style={{
            width: `${percent}%`,
            backgroundSize: `${span}% 100%`,
          }}
        />
      </div>
      <span className="w-12 shrink-0 text-right tabular-nums text-chrome-menu-text">
        {percent.toFixed(2)}%
      </span>
    </div>
  );
}
