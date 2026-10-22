import { Dialog } from "@/components/ui/dialog";
import { useDownloads } from "@/state/download-store";

/**
 * Confirms a batch delete. A single row deletes straight away when the user
 * picked it themselves; a batch gets a question because it is one keystroke
 * away from losing a lot, and "Delete Completed" always asks because its scope
 * is not the row under the pointer.
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
          <button
            type="button"
            onClick={cancelDelete}
            className="rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirmDelete}
            className="rounded-chrome bg-chrome-danger px-3 py-1 text-xs font-medium text-chrome-danger-text hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            {one ? "Remove" : `Remove ${count}`}
          </button>
        </>
      }
    >
      <p className="text-xs text-chrome-menu-text">
        Remove {one ? "1 download" : `${count} downloads`} from the list? Files
        already saved to disk are not deleted.
      </p>
    </Dialog>
  );
}
