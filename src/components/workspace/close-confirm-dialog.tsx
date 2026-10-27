import { WarningMark } from "@/components/icons/desktop-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";

interface CloseConfirmDialogProps {
  /** The downloads that would be paused by leaving now. */
  active: number;
  /** Hide the window and keep them going from the tray. */
  onKeepRunning: () => void;
  /** Save their progress, pause them and leave. */
  onExit: () => void;
  /** Stay as we are; nothing changes. */
  onCancel: () => void;
}

/**
 * The question a close button asks when quitting would interrupt downloads.
 *
 * The safe answer has the focus, and it is the one that keeps the application
 * alive: hiding the window costs nothing and leaves the tray icon in reach.
 * Exiting is still one press away, but it saves and pauses first, so it is
 * never an accidental loss of progress.
 */
export function CloseConfirmDialog({
  active,
  onKeepRunning,
  onExit,
  onCancel,
}: CloseConfirmDialogProps) {
  const one = active === 1;
  return (
    <Dialog
      open
      title="Downloads are still running"
      onClose={onCancel}
      footer={
        <>
          <DialogAction tone="quiet" onClick={onCancel}>
            Cancel
          </DialogAction>
          <DialogAction tone="danger" onClick={onExit}>
            Exit anyway
          </DialogAction>
          <DialogAction tone="primary" data-autofocus onClick={onKeepRunning}>
            Keep running
          </DialogAction>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="flex items-start gap-1.5 text-xs text-chrome-menu-text">
          <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
          <span className="min-w-0">
            {one
              ? "One download is still running."
              : `${active} downloads are still running.`}{" "}
            Keep them going from the tray, or exit; exiting pauses them and
            remembers where they stopped.
          </span>
        </p>
        <p className="text-chrome-micro text-chrome-muted">
          The tray icon brings the window back, and its menu can pause or
          resume the downloads.
        </p>
      </div>
    </Dialog>
  );
}
