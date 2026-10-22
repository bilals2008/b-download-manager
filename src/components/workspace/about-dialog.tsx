import { AppMark } from "@/components/shell/app-mark";
import { Dialog } from "@/components/ui/dialog";

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
}

/** "About B Download Manager" - the box behind Help. */
export function AboutDialog({ open, onClose }: AboutDialogProps) {
  return (
    <Dialog
      open={open}
      title="About B Download Manager"
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
        <div className="flex items-center gap-3">
          <AppMark className="size-10" />
          <p className="text-sm text-chrome-title-text">B Download Manager 0.1.0</p>
        </div>
        <p>
          A multi-connection download manager. The Rust engine behind this
          window splits each file into ranged chunks, resumes from its manifest
          and reports its real size, speed and progress to this list.
        </p>
        <p className="text-chrome-muted">
          A study of the Internet Download Manager 6.43 interface, which is a
          trademark of Tonec Inc.
        </p>
      </div>
    </Dialog>
  );
}
