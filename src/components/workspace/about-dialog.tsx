import { AuthorAvatar } from "@/components/shell/author-avatar";
import { AppMark } from "@/components/shell/app-mark";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { useAppVersion } from "@/lib/app-version";
import { APP_NAME, APP_TAGLINE } from "@/lib/brand";

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * "About B Download Manager" - the box behind Help.
 *
 * The version comes from the built application itself, so this box and the
 * installer always name the same number; nothing here is typed by hand. The
 * options window reads the same value on its About page.
 */
export function AboutDialog({ open, onClose }: AboutDialogProps) {
  const version = useAppVersion();

  return (
    <Dialog
      open={open}
      title={`About ${APP_NAME}`}
      onClose={onClose}
      footer={
        <DialogAction tone="primary" data-autofocus onClick={onClose}>
          OK
        </DialogAction>
      }
    >
      <div className="flex flex-col gap-2 text-xs text-chrome-menu-text">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <AppMark className="size-10" />
            <p className="text-sm text-chrome-title-text">
              {APP_NAME}
              {version ? ` ${version}` : ""}
            </p>
          </div>
          <AuthorAvatar />
        </div>
        <p>{APP_TAGLINE}</p>
      </div>
    </Dialog>
  );
}
