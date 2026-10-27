import type { ReactNode } from "react";

import {
  OpenFileGlyph,
  PauseGlyph,
  PlayGlyph,
  StopGlyph,
} from "@/components/icons/action-icons";
import { WarningMark } from "@/components/icons/desktop-icons";
import { CloseGlyph, ChevronDown, ChevronUp } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import { ConnectionDetails } from "@/components/windows/progress/connection-details";
import { ProgressMeter } from "@/components/windows/progress/progress-meter";
import {
  formatEta,
  formatSize,
  formatSpeed,
} from "@/lib/downloads";
import {
  limitLabel,
  percentOf,
  progressControls,
  progressPercentText,
  resumeLabel,
  statusLabel,
} from "@/lib/progress";
import { cn } from "@/lib/utils";
import type { DownloadInfo, DownloadStatus } from "@/types";

export type ProgressAction = "pause" | "resume" | "cancel";

/** The dot beside the status word: green only for finished work. */
const STATUS_DOT: Record<DownloadStatus, string> = {
  queued: "bg-chrome-muted",
  running: "bg-progress",
  paused: "bg-chrome-muted",
  completed: "bg-success",
  failed: "bg-destructive",
  cancelled: "bg-chrome-muted",
};

interface DownloadStatusTabProps {
  info: DownloadInfo;
  /** Smoothed bytes per second, from the window's own samples. */
  speed: number;
  /** Seconds left at that rate, or 0 when it is not known. */
  eta: number;
  detailsOpen: boolean;
  onToggleDetails: () => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  /** Open the finished file, the way the completion window does. */
  onOpen: () => void;
  /** Close this window without stopping the download. */
  onClose: () => void;
  /** The action in flight, if any. Every control waits for it. */
  busy: ProgressAction | null;
  /** A refusal from the engine, shown above the controls. */
  actionError: string | null;
}

/**
 * The Download status page: the state of the transfer, the figures the engine
 * reports, the three controls, and the connections the Hide details control
 * folds.
 *
 * The controls come from `progressControls`, so which of them can act is a
 * property of the download, not a guess made by three separate conditions in
 * the markup. A finished download offers its file; a stopped one offers a
 * resume and no second stop.
 */
export function DownloadStatusTab({
  info,
  speed,
  eta,
  detailsOpen,
  onToggleDetails,
  onPause,
  onResume,
  onCancel,
  onOpen,
  onClose,
  busy,
  actionError,
}: DownloadStatusTabProps) {
  const percent = percentOf(info);
  const percentTextValue = progressPercentText(info);
  const knownSize = info.total > 0;
  const running = info.status === "running";
  const controls = progressControls(info);
  const working = busy !== null;
  // A refusal of the last command is newer than the failure the engine
  // reported, so it is the one the window shows; both are kept out of the
  // height budget with two lines and a tooltip.
  const problem = actionError ?? info.error;

  return (
    <section
      role="tabpanel"
      id="progress-panel-status"
      aria-labelledby="progress-tab-status"
      className="flex min-h-0 flex-1 flex-col gap-3"
    >
      <div className="flex items-center justify-between gap-3">
        <span className="inline-flex min-w-0 items-center gap-2 text-xs text-chrome-menu-text">
          <span
            aria-hidden="true"
            className={cn(
              "size-1.5 shrink-0 rounded-full",
              STATUS_DOT[info.status],
              running && "motion-safe:animate-pulse",
            )}
          />
          {/* The status changes rarely, so it is the one thing worth
              announcing while the percentage changes every event. */}
          <span aria-live="polite" className="truncate">
            {statusLabel(info.status)}
          </span>
        </span>
        <span className="shrink-0 text-sm tabular-nums text-chrome-menu-text">
          {percentTextValue}
        </span>
      </div>

      <ProgressMeter
        value={percent}
        // A finished file is complete even when the server never told its
        // size: the meter fills with the success colour instead of running.
        unknown={!knownSize && info.status !== "completed"}
        status={info.status}
        segments={info.segments}
        total={info.total}
      />

      <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-6 gap-y-1.5 text-xs">
        <Field label="File size">
          {knownSize ? formatSize(info.total) : "Not known"}
        </Field>
        <Field label="Downloaded">
          {knownSize
            ? `${formatSize(info.downloaded)} (${percentTextValue})`
            : formatSize(info.downloaded)}
        </Field>
        <Field label="Transfer rate">
          {running && speed > 0 ? formatSpeed(speed) : "—"}
        </Field>
        <Field label="Time left">
          {running && eta > 0 ? formatEta(eta) : "—"}
        </Field>
        <Field label="Resume capability">
          {info.status === "completed" ? "—" : resumeLabel(info.resumable)}
        </Field>
        <Field label="Speed limit">{limitLabel(info.speedLimit)}</Field>
      </dl>

      {problem ? (
        <Tooltip content={problem} side="top" className="block">
          <p
            role="alert"
            className="flex items-start gap-1.5 text-chrome-micro text-chrome-error"
          >
            <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
            <span className="line-clamp-2 min-w-0 break-words">{problem}</span>
          </p>
        </Tooltip>
      ) : null}

      <div className="mt-auto flex items-center justify-between gap-2 pt-0.5">
        <DialogAction
          tone="quiet"
          onClick={onToggleDetails}
          aria-expanded={detailsOpen}
          aria-controls={detailsOpen ? "progress-connections" : undefined}
          icon={
            detailsOpen ? (
              <ChevronUp className="size-3.5" />
            ) : (
              <ChevronDown className="size-3.5" />
            )
          }
        >
          {detailsOpen ? "Hide details" : "Show details"}
        </DialogAction>

        {controls.primary === "open" ? (
          <div className="flex items-center gap-2">
            <DialogAction
              tone="secondary"
              icon={<CloseGlyph className="size-3.5" />}
              onClick={onClose}
            >
              Close
            </DialogAction>
            <Tooltip content={controls.primaryHint} side="top">
              <DialogAction
                tone="primary"
                icon={<OpenFileGlyph className="size-3.5" />}
                onClick={onOpen}
                disabled={working}
              >
                {controls.primaryLabel}
              </DialogAction>
            </Tooltip>
          </div>
        ) : (
          <div className="flex items-center gap-2">
            <Tooltip content={controls.cancelHint} side="top">
              <DialogAction
                tone="danger"
                icon={<StopGlyph className="size-3.5" />}
                onClick={onCancel}
                disabled={working || !controls.canCancel}
              >
                Cancel
              </DialogAction>
            </Tooltip>

            {controls.canPause ? (
              <Tooltip content={controls.primaryHint} side="top">
                <DialogAction
                  tone="secondary"
                  icon={<PauseGlyph className="size-3.5" />}
                  onClick={onPause}
                  disabled={working}
                >
                  {controls.primaryLabel}
                </DialogAction>
              </Tooltip>
            ) : (
              <Tooltip content={controls.primaryHint} side="top">
                <DialogAction
                  tone="primary"
                  icon={<PlayGlyph className="size-3.5" />}
                  onClick={onResume}
                  disabled={working || !controls.canResume}
                >
                  {controls.primaryLabel}
                </DialogAction>
              </Tooltip>
            )}
          </div>
        )}
      </div>

      {detailsOpen ? (
        // The panel unfolds as the window grows into it: the height travels on
        // the shell side, so the content only has to step out of its own way.
        <div className="flex min-h-0 flex-1 flex-col motion-safe:animate-details-in motion-reduce:animate-none">
          <ConnectionDetails segments={info.segments} status={info.status} />
        </div>
      ) : null}
    </section>
  );
}

/** A label and its value: the engine said it, so the value is monospaced digits. */
function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-chrome-muted">{label}</dt>
      <dd className="min-w-0 truncate text-right tabular-nums text-chrome-menu-text">
        {children}
      </dd>
    </>
  );
}
