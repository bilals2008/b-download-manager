import {
  InfoGlyph,
  TrashGlyph,
} from "@/components/icons/action-icons";
import { CheckGlyph, PlusGlyph } from "@/components/icons/ui-icons";
import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import type { CategoryName, DownloadFilter } from "@/lib/downloads";
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
  /** True when the category was made by the user, so it can be removed. */
  custom: boolean;
  /** Where the pointer was, in window coordinates. */
  at: { x: number; y: number };
  onAddCategory: () => void;
  onEditCategory: (name: CategoryName) => void;
  onRemoveCategory: (name: CategoryName) => void;
  onClose: () => void;
}

/**
 * The menu of a category in the tree.
 *
 * A built-in keeps its name and its file types; its folder is still the user's
 * to choose. Only a category the user made can be removed, and removing one
 * moves its rows to Other instead of deleting anything on disk.
 */
export function CategoryContextMenu({
  filter,
  isFolder,
  custom,
  at,
  onAddCategory,
  onEditCategory,
  onRemoveCategory,
  onClose,
}: CategoryContextMenuProps) {
  const { selectByFilter } = useDownloads();

  function choose(command: CategoryCommand) {
    if (command === "select-all") {
      selectByFilter(filter);
    } else if (command === "add-category") {
      onAddCategory();
    } else if (command === "category-properties" && isFolder) {
      onEditCategory(filter);
    } else if (command === "remove-category" && isFolder && custom) {
      onRemoveCategory(filter);
    }
    onClose();
  }

  const rows: MenuRow<CategoryCommand>[] = [
    {
      command: "select-all",
      label: "Select all downloads",
      icon: <CheckGlyph className="size-3.5" />,
    },
    "separator",
    {
      command: "add-category",
      label: "Add category...",
      icon: <PlusGlyph className="size-3.5" />,
    },
  ];

  if (isFolder) {
    rows.push(
      {
        command: "category-properties",
        label: "Category properties...",
        icon: <InfoGlyph className="size-3.5" />,
      },
      {
        command: "remove-category",
        label: "Remove category",
        icon: <TrashGlyph className="size-3.5" />,
        danger: true,
        disabled: !custom,
      },
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
