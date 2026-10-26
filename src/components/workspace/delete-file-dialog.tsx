import { useRef, useState } from "react";

import { WarningMark } from "@/components/icons/desktop-icons";
import { SpinnerGlyph } from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { type Download } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { useDownloads } from "@/state/download-store";

interface DeleteFileDialogProps {
  item: Download;
  onClose: () => void;
}

/**
 * Ask before a finished file leaves the disk.
 *
 * Removing a row only forgets it; this is the one command that deletes the
 * bytes, so it says both things and starts the keyboard on Cancel. The row is
 * dropped once the file is gone, never before.
 */
export function DeleteFileDialog({ item, onClose }: DeleteFileDialogProps) {
  const { deleteFile } = useDownloads();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);

  async function confirm() {
    if (running.current) {
      return;
    }
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      await deleteFile(item.id);
      onClose();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  return (
    <Dialog
      open
      title="Delete file"
      onClose={busy ? () => undefined : onClose}
      dismissible={!busy}
      footer={
        <>
          <DialogAction data-autofocus onClick={onClose} disabled={busy}>
            Cancel
          </DialogAction>
          <DialogAction
            tone="danger"
            onClick={() => void confirm()}
            disabled={busy}
            icon={
              busy ? (
                <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
              ) : undefined
            }
          >
            {busy ? "Deleting…" : "Delete file"}
          </DialogAction>
        </>
      }
    >
      <p className="text-xs text-chrome-menu-text">
        Delete <span className="font-medium">{item.fileName}</span> from the disk
        and remove it from the list? This cannot be undone.
      </p>
      <p className="mt-2 font-mono text-chrome-micro break-all text-chrome-muted">
        {item.dest}
      </p>
      {error ? (
        <p
          role="alert"
          className="mt-3 flex items-start gap-1.5 text-xs text-chrome-error"
        >
          <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
          <span className="min-w-0 break-words">{error}</span>
        </p>
      ) : null}
    </Dialog>
  );
}
