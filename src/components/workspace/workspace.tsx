import { CategoriesPane } from "@/components/workspace/categories-pane";
import { DeleteConfirmDialog } from "@/components/workspace/delete-confirm-dialog";
import { DownloadList } from "@/components/workspace/download-list";
import { StatusBar } from "@/components/workspace/status-bar";

interface WorkspaceProps {
  /** The View menu hides the categories pane in IDM, so it is app state. */
  categoriesVisible: boolean;
  /** Opens the "Add new download" dialog from the empty state. */
  onAddDownload: () => void;
}

/**
 * The area below the toolbar: the categories pane, the download list and the
 * status bar.
 */
export function Workspace({ categoriesVisible, onAddDownload }: WorkspaceProps) {
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
    </main>
  );
}
