import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { useDownloads } from "@/state/download-store";

/**
 * Confirms a batch delete. A single row deletes straight away when the user
 * picked it themselves; a batch gets a question because it is one keystroke
 * away from losing a lot, and "Delete Completed" always asks because its scope
 * is not the row under the pointer.
 *
 * The keyboard starts on Cancel: Enter on a question about removing downloads
 * should not remove anything.
 */
export function DeleteConfirmDialog() {
  const { pendingDelete, confirmDelete, cancelDelete } = useDownloads();
  const count = pendingDelete?.length ?? 0;
  const one = count === 1;

  return (
    <Dialog
      open={count > 0}
      title={one ? "Remove download" : "Remove downloads"}
      onClose={cancelDelete}
      footer={
        <>
          <DialogAction data-autofocus onClick={cancelDelete}>
            Cancel
          </DialogAction>
          <DialogAction tone="danger" onClick={confirmDelete}>
            {one ? "Remove" : `Remove ${count}`}
          </DialogAction>
        </>
      }
    >
      <p className="text-xs text-chrome-menu-text">
        Remove {one ? "1 download" : `${count} downloads`} from the list? A
        finished file is kept on the disk; the parts of an unfinished download
        are deleted.
      </p>
    </Dialog>
  );
}
