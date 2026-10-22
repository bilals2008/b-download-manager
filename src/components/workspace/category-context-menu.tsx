import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import type { DownloadFilter } from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

/** What the menu of a category can do. */
type CategoryCommand =
  | "select-all"
  | "add-category"
  | "category-properties"
  | "remove-category";

interface CategoryContextMenuProps {
  /** The category, or the group, the menu was opened on. */
  filter: DownloadFilter;
  /** True for a category folder, false for the three groups above them. */
  isFolder: boolean;
  /** Where the pointer was, in window coordinates. */
  at: { x: number; y: number };
  onClose: () => void;
}

/**
 * The menu of a category in the tree.
 *
 * Custom categories are not built yet, so those entries stay greyed out
 * instead of pretending to work.
 */
export function CategoryContextMenu({
  filter,
  isFolder,
  at,
  onClose,
}: CategoryContextMenuProps) {
  const { selectByFilter } = useDownloads();

  function choose(command: CategoryCommand) {
    if (command === "select-all") {
      selectByFilter(filter);
    }
    onClose();
  }

  const rows: MenuRow<CategoryCommand>[] = [
    { command: "select-all", label: "Select all downloads" },
    "separator",
    { command: "add-category", label: "Add category...", disabled: true },
  ];

  if (isFolder) {
    rows.push(
      { command: "category-properties", label: "Category properties...", disabled: true },
      { command: "remove-category", label: "Remove category", disabled: true },
    );
  }

  return (
    <MenuPopup
      id="category-context-menu"
      label="Category actions"
      rows={rows}
      level={0}
      focus="first"
      at={at}
      onChoose={(row) => choose(row.command)}
      onClose={onClose}
    />
  );
}
