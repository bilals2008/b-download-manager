import { useEffect, useState } from "react";

import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { WarningMark } from "@/components/icons/desktop-icons";
import { APP_NAME } from "@/lib/brand";
import {
  describeFailure,
  formatReleaseDate,
  installUpdate,
  outcomeOf,
  runCheck,
  type CheckOutcome,
  type UpdateFound,
} from "@/lib/updates";

interface UpdateDialogProps {
  /** False while the window is closed; opening it starts one check. */
  open: boolean;
  /**
   * An answer the caller already has, from a check made before this window
   * was asked to show it. The startup check uses it, so one question to the
   * release server stays one question.
   */
  found?: UpdateFound;
  onClose: () => void;
}

/** Where the check is between the reader pressing the menu item and the end. */
type Stage =
  | { kind: "checking" }
  | { kind: "found"; update: UpdateFound }
  | { kind: "latest"; version: string }
  | { kind: "error"; message: string }
  | { kind: "installing" };

/** The stage an answer puts the window on. */
function stageOf(outcome: CheckOutcome): Stage {
  return outcome.kind === "found"
    ? { kind: "found", update: outcome.update }
    : outcome.kind === "latest"
      ? { kind: "latest", version: outcome.version }
      : { kind: "error", message: outcome.message };
}

/**
 * "Check for updates" - the one question the release endpoint answers.
 *
 * Nothing is fetched or run that the reader has not asked for, and the
 * package itself is verified against the application's public key before
 * the installer starts. The notes come from the release, so the reader sees
 * what is actually coming before saying yes.
 */
export function UpdateDialog({ open, found, onClose }: UpdateDialogProps) {
  const [stage, setStage] = useState<Stage>({ kind: "checking" });

  // One check per opening: the menu item is the reader asking, so honour it
  // as soon as the window appears, and do not re-ask on re-renders. An answer
  // the caller brought along is shown as it is instead of being asked again.
  useEffect(() => {
    if (!open) return;
    if (found) {
      setStage(stageOf(outcomeOf(found)));
      return;
    }
    let alive = true;
    setStage({ kind: "checking" });
    void runCheck().then((outcome) => {
      if (alive) {
        setStage(stageOf(outcome));
      }
    });
    return () => {
      alive = false;
    };
  }, [open, found]);

  const title =
    stage.kind === "found"
      ? `Version ${stage.update.version} is available`
      : "Check for updates";

  const footer =
    stage.kind === "found" ? (
      <>
        <DialogAction tone="quiet" onClick={onClose}>
          Not now
        </DialogAction>
        <DialogAction
          tone="primary"
          data-autofocus
          onClick={() => {
            setStage({ kind: "installing" });
            // Resolves only when something went wrong; on success the
            // installer takes the application over.
            void installUpdate().then(
              () => {},
              (error: unknown) =>
                setStage({ kind: "error", message: describeFailure(error) }),
            );
          }}
        >
          Download and install
        </DialogAction>
      </>
    ) : (
      <DialogAction tone="primary" data-autofocus onClick={onClose}>
        {stage.kind === "installing" ? "Waiting for the installer" : "Close"}
      </DialogAction>
    );

  return (
    <Dialog open={open} title={title} onClose={onClose} footer={footer}>
      <div className="flex flex-col gap-2 text-xs text-chrome-menu-text">
        {stage.kind === "checking" && <p>Looking for a newer version…</p>}
        {stage.kind === "installing" && (
          <p>
            The new version is downloaded and checked that it is genuine. The
            installer takes over from here; downloads that were running will
            start again when the new version opens.
          </p>
        )}
        {stage.kind === "latest" && (
          <p>{APP_NAME} {stage.version} is the newest version.</p>
        )}
        {stage.kind === "error" && (
          <p className="flex items-start gap-1.5">
            <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0">{stage.message}</span>
          </p>
        )}
        {stage.kind === "found" && (
          <>
            <p>
              Version {stage.update.version}
              {formatReleaseDate(stage.update.date)
                ? `, released ${formatReleaseDate(stage.update.date)}`
                : ""}
              . You are on {stage.update.currentVersion}.
            </p>
            {stage.update.notes && (
              <p className="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-(--radius-sm) border border-chrome-border bg-chrome-panel p-2 text-chrome-title-text">
                {stage.update.notes}
              </p>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}
