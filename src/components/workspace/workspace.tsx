import { CategoriesPane } from "@/components/workspace/categories-pane";
import { DownloadList } from "@/components/workspace/download-list";
import { StatusBar } from "@/components/workspace/status-bar";

interface WorkspaceProps {
  /** The View menu hides the categories pane in IDM, so it is app state. */
  categoriesVisible: boolean;
}

/**
 * The area below the toolbar: the categories pane, the download list and the
 * status bar.
 */
export function Workspace({ categoriesVisible }: WorkspaceProps) {
  return (
    <main
      aria-label="Download workspace"
      className="flex min-h-0 flex-1 flex-col"
    >
      <div className="flex min-h-0 flex-1">
        {categoriesVisible ? <CategoriesPane /> : null}
        <DownloadList />
      </div>
      <StatusBar />
    </main>
  );
}
