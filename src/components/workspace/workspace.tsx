import { CategoriesPane } from "@/components/workspace/categories-pane";
import { DeleteConfirmDialog } from "@/components/workspace/delete-confirm-dialog";
import { DownloadList } from "@/components/workspace/download-list";
import { FileExistsDialog } from "@/components/workspace/file-exists-dialog";
import { StatusBar } from "@/components/workspace/status-bar";
import { useDownloads } from "@/state/download-store";

interface WorkspaceProps {
  /** The View menu can hide the categories pane, so it is app state. */
  categoriesVisible: boolean;
  /** Opens the "Add new download" dialog from the empty state. */
  onAddDownload: () => void;
}

/**
 * The area below the toolbar: the categories pane, the download list and the
 * status bar.
 */
export function Workspace({ categoriesVisible, onAddDownload }: WorkspaceProps) {
  const { conflict, answerConflict, cancelConflict } = useDownloads();

  return (
    <main
      aria-label="Download workspace"
      className="flex min-h-0 flex-1 flex-col"
    >
      <div className="flex min-h-0 flex-1 gap-1.5 px-1.5 pt-1.5 pb-1.5">
        {categoriesVisible ? <CategoriesPane /> : null}
        <DownloadList onAddDownload={onAddDownload} />
      </div>
      <StatusBar />
      <DeleteConfirmDialog />
      {/* Keyed by the file it asks about, so a second question never inherits
          the checkbox of the first. */}
      {conflict ? (
        <FileExistsDialog
          key={`${conflict.path}:${conflict.remaining}`}
          conflict={conflict}
          onAnswer={answerConflict}
          onCancel={cancelConflict}
        />
      ) : null}
    </main>
  );
}
