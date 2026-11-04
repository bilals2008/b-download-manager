import { WarningMark } from "@/components/icons/desktop-icons";
import {
  gradientSpan,
  PROGRESS_FILL,
} from "@/components/ui/progress-fill";
import { formatSpeed } from "@/lib/downloads";
import {
  updatePercent,
  updateSize,
  updateStageLabel,
  type UpdateStage,
} from "@/lib/updates";
import { cn } from "@/lib/utils";
import type { UpdateRun } from "@/state/update-store";

/** The bar of an update: a real byte share, or a moving segment if unknown. */
function UpdateMeter({
  percent,
  stage,
  className,
}: {
  /** Null while the server has not named the package size. */
  percent: number | null;
  stage: UpdateStage;
  className?: string;
}) {
  const known = percent !== null;
  const value = percent ?? 0;

  return (
    <div
      role="progressbar"
      aria-label="Update progress"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={known ? value : undefined}
      aria-valuetext={
        known ? `${value}%` : `${updateStageLabel(stage)}, size unknown`
      }
      className={cn(
        "relative h-1.5 w-full overflow-hidden rounded-full bg-progress-track",
        className,
      )}
    >
      {known ? (
        <div
          aria-hidden="true"
          className={cn(
            "h-full rounded-full bg-no-repeat transition-[width] duration-300 ease-out motion-reduce:transition-none",
            PROGRESS_FILL.active,
          )}
          style={{
            width: `${value}%`,
            backgroundSize: `${gradientSpan(value)}% 100%`,
          }}
        />
      ) : (
        <div
          aria-hidden="true"
          className={cn(
            "absolute inset-y-0 left-0 w-1/3 rounded-full motion-safe:animate-indeterminate motion-reduce:left-0 motion-reduce:w-1/2",
            PROGRESS_FILL.active,
          )}
        />
      )}
    </div>
  );
}

/**
 * The one picture of a running update: stage, real bytes, and the bar.
 *
 * The update notice, the update dialog and the About page show this same
 * block, so none of them can invent a percentage the backend did not send.
 */
export function UpdateProgressBody({
  run,
  className,
}: {
  run: UpdateRun;
  className?: string;
}) {
  if (run.phase === "idle") {
    return null;
  }

  if (run.phase === "error") {
    return (
      <div className={cn("flex items-start gap-1.5 text-xs", className)}>
        <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
        <span className="min-w-0 text-chrome-error">
          {run.error ?? "The update could not be downloaded."}
        </span>
      </div>
    );
  }

  if (run.phase === "restart-ready") {
    return (
      <p className={cn("text-xs text-chrome-menu-text", className)}>
        The new version is installed. Restart to finish.
      </p>
    );
  }

  const percent = updatePercent(run.downloaded, run.total);
  const size = updateSize(run.downloaded, run.total);
  const speed =
    run.phase === "downloading" && run.speed ? formatSpeed(run.speed) : "";
  const detail =
    run.phase === "downloading"
      ? size
      : run.phase === "verifying"
        ? "The download is complete; its signature is being checked."
        : "The verified package is being installed.";

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex items-baseline justify-between gap-3 text-xs">
        <span className="text-chrome-title-text">
          {updateStageLabel(run.phase)}
        </span>
        {run.phase === "downloading" && percent !== null ? (
          <span className="shrink-0 text-chrome-micro tabular-nums text-chrome-muted">
            {percent}%
          </span>
        ) : null}
      </div>
      <UpdateMeter
        stage={run.phase}
        percent={
          run.phase === "downloading" ? percent : run.total ? 100 : null
        }
      />
      <p className="text-chrome-micro leading-4 text-chrome-muted">
        {`${detail}${speed ? ` · ${speed}` : ""}`}
      </p>
    </div>
  );
}
