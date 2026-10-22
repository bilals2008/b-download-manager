import { Dialog } from "@/components/ui/dialog";

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * "About IDM..." - the box behind Help, About IDM.
 */
export function AboutDialog({ open, onClose }: AboutDialogProps) {
  return (
    <Dialog
      open={open}
      title="About IDM"
      onClose={onClose}
      footer={
        <button
          type="button"
          onClick={onClose}
          className="rounded-chrome bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          OK
        </button>
      }
    >
      <div className="flex flex-col gap-2 text-xs text-chrome-menu-text">
        <p className="text-sm text-chrome-title-text">Internet Download Manager 6.43</p>
        <p>
          A rebuild of the IDM 6.43 interface, written from the strings and the
          behaviour of the original program. The Rust engine behind this window
          splits each file into ranged chunks, resumes from its manifest and
          reports its real size, speed and progress to this list.
        </p>
        <p className="text-chrome-muted">
          Internet Download Manager is a trademark of Tonec Inc. This is an
          independent study of its user interface.
        </p>
      </div>
    </Dialog>
  );
}
