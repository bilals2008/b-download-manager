import { useEffect, useState } from "react";

import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { WarningMark } from "@/components/icons/desktop-icons";
import { ReleaseNotes } from "@/components/ui/release-notes";
import { UpdateProgressBody } from "@/components/updates/update-meter";
import { APP_NAME } from "@/lib/brand";
import {
  formatReleaseDate,
  runCheck,
  type CheckOutcome,
  type UpdateFound,
} from "@/lib/updates";
import {
  beginUpdate,
  clearUpdateRun,
  currentUpdateRun,
  restartApplication,
  retryUpdate,
  useUpdateRun,
} from "@/state/update-store";

interface UpdateDialogProps {
  /** False while the window is closed; opening it starts one check. */
  open: boolean;
  onClose: () => void;
}

/** Where the check is between the reader pressing the menu item and the end. */
type Stage =
  | { kind: "checking" }
  | { kind: "found"; update: UpdateFound }
  | { kind: "latest"; version: string }
  | { kind: "error"; message: string };

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
 * package itself is verified against the application's public key before the
 * installer starts. Once the reader says yes, this window shows the backend's
 * own byte count and stage instead of a spinner: it never claims an update
 * finished before the installer confirms it.
 */
export function UpdateDialog({ open, onClose }: UpdateDialogProps) {
  const [stage, setStage] = useState<Stage>({ kind: "checking" });
  const run = useUpdateRun();
  const applying =
    run.phase === "downloading" ||
    run.phase === "verifying" ||
    run.phase === "installing" ||
    run.phase === "restart-ready" ||
    run.phase === "error";

  // One check per opening: the menu item is the reader asking. A download
  // that is already running owns the window instead of asking the server a
  // second question about the release being installed.
  useEffect(() => {
    if (!open) return;
    if (currentUpdateRun().phase !== "idle") {
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
  }, [open]);

  const found = stage.kind === "found" ? stage.update : null;
  const title = applying
    ? `Updating ${APP_NAME}`
    : found
      ? `Version ${found.version} is available`
      : stage.kind === "latest"
        ? "You are up to date"
        : "Check for updates";

  const footer = applying ? (
    run.phase === "restart-ready" ? (
      <>
        <DialogAction
          tone="quiet"
          onClick={() => {
            clearUpdateRun();
            onClose();
          }}
        >
          Later
        </DialogAction>
        <DialogAction tone="primary" data-autofocus onClick={restartApplication}>
          Restart now
        </DialogAction>
      </>
    ) : run.phase === "error" ? (
      <>
        <DialogAction
          tone="quiet"
          onClick={() => {
            clearUpdateRun();
            onClose();
          }}
        >
          Later
        </DialogAction>
        <DialogAction tone="primary" data-autofocus onClick={retryUpdate}>
          Try again
        </DialogAction>
      </>
    ) : (
      <DialogAction tone="quiet" onClick={onClose}>
        Close
      </DialogAction>
    )
  ) : found ? (
    <>
      <DialogAction tone="quiet" onClick={onClose}>
        Not now
      </DialogAction>
      <DialogAction
        tone="primary"
        data-autofocus
        onClick={() => beginUpdate(found)}
      >
        Update now
      </DialogAction>
    </>
  ) : stage.kind === "error" ? (
    <>
      <DialogAction tone="quiet" onClick={onClose}>
        Close
      </DialogAction>
      <DialogAction
        tone="primary"
        data-autofocus
        onClick={() => {
          setStage({ kind: "checking" });
          void runCheck().then((outcome) => setStage(stageOf(outcome)));
        }}
      >
        Try again
      </DialogAction>
    </>
  ) : (
    <DialogAction tone="primary" data-autofocus onClick={onClose}>
      Close
    </DialogAction>
  );

  return (
    <Dialog
      open={open}
      title={title}
      onClose={onClose}
      footer={footer}
      className="w-[28rem] max-w-[calc(100vw-2rem)]"
      bodyClassName="flex min-h-0 flex-col"
    >
      <div className="flex min-h-0 flex-col gap-2 text-xs text-chrome-menu-text">
        {applying ? (
          <>
            <p>
              Version {run.update?.version ?? ""}
              {run.update?.currentVersion
                ? `, on this machine ${run.update.currentVersion}`
                : ""}
              .
            </p>
            <UpdateProgressBody run={run} />
            {run.phase === "downloading" || run.phase === "verifying" ? (
              <p className="text-chrome-muted">
                Keep the application open. Running downloads continue until
                the installer takes over.
              </p>
            ) : null}
          </>
        ) : stage.kind === "checking" ? (
          <p>Looking for a newer version…</p>
        ) : stage.kind === "latest" ? (
          <p>
            {APP_NAME} {stage.version} is the newest version.
          </p>
        ) : stage.kind === "error" ? (
          <p className="flex items-start gap-1.5">
            <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0">{stage.message}</span>
          </p>
        ) : (
          <>
            <p>
              Version {stage.update.version}
              {formatReleaseDate(stage.update.date)
                ? `, released ${formatReleaseDate(stage.update.date)}`
                : ""}
              . You are on {stage.update.currentVersion}.
            </p>
            {stage.update.notes ? (
              <div className="max-h-60 overflow-y-auto scrollbar-thin rounded-(--radius-sm) bg-chrome-menu-hover p-2">
                <ReleaseNotes
                  notes={stage.update.notes}
                  className="text-chrome-title-text"
                />
              </div>
            ) : null}
          </>
        )}
      </div>
    </Dialog>
  );
}
