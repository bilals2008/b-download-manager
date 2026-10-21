import { useRef, useState, type ComponentType } from "react";
import {
  Archive,
  CircleCheck,
  CircleDashed,
  FileText,
  Film,
  Folder,
  Gamepad2,
  ListOrdered,
  Music,
  Package,
  Shapes,
  type LucideProps,
} from "lucide-react";

import {
  CATEGORIES,
  type CategoryName,
  type DownloadFilter,
} from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

type Icon = ComponentType<LucideProps>;

interface Entry {
  id: DownloadFilter;
  label: string;
  icon: Icon;
}

const GROUPS: Entry[] = [
  { id: "all", label: "All Downloads", icon: ListOrdered },
  { id: "unfinished", label: "Unfinished", icon: CircleDashed },
  { id: "finished", label: "Finished", icon: CircleCheck },
];

const CATEGORY_ICONS: Record<CategoryName, Icon> = {
  General: Folder,
  Video: Film,
  Music: Music,
  Documents: FileText,
  Programs: Package,
  Games: Gamepad2,
  Compressed: Archive,
  Other: Shapes,
};

/**
 * The categories pane. It is a tree: the groups from IDM's own tree sit on
 * top, the download categories below them, and every entry carries the number
 * of downloads behind it.
 */
export function CategoriesPane() {
  const { filter, setFilter, counts } = useDownloads();
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const entries: Entry[] = [
    ...GROUPS,
    ...CATEGORIES.map((category) => ({
      id: category as DownloadFilter,
      label: category,
      icon: CATEGORY_ICONS[category],
    })),
  ];

  function focusItem(index: number) {
    const next = (index + entries.length) % entries.length;
    setFocusIndex(next);
    itemRefs.current[next]?.focus();
  }
  function handleKeyDown(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focusItem(index + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        focusItem(index - 1);
        break;
      case "Home":
        event.preventDefault();
        focusItem(0);
        break;
      case "End":
        event.preventDefault();
        focusItem(entries.length - 1);
        break;
    }
  }

  const selectedIndex = Math.max(
    0,
    entries.findIndex((entry) => entry.id === filter),
  );
  const rovingIndex = focusIndex ?? selectedIndex;

  return (
    <nav
      aria-label="Categories"
      className="flex w-57 shrink-0 flex-col border-r border-chrome-line bg-chrome-panel"
    >
      <ul
        role="tree"
        aria-label="Download categories"
        className="flex flex-col gap-0.5 overflow-y-auto p-1.5"
      >
        {entries.map((entry, index) => {
          const Icon = entry.icon;
          const isSelected = entry.id === filter;
          const count = counts[entry.id] ?? 0;
          return (
            <li key={entry.id} role="none">
              <button
                ref={(node) => {
                  itemRefs.current[index] = node;
                }}
                type="button"
                role="treeitem"
                aria-selected={isSelected}
                tabIndex={index === rovingIndex ? 0 : -1}
                onClick={() => {
                  setFocusIndex(index);
                  setFilter(entry.id);
                }}
                onFocus={() => setFocusIndex(index)}
                onKeyDown={(event) => handleKeyDown(event, index)}
                className={
                  "flex w-full items-center gap-2 rounded-chrome px-2 py-1.5 text-left text-xs " +
                  (isSelected
                    ? "bg-chrome-menu-active text-chrome-title-text"
                    : "text-chrome-menu-text hover:bg-chrome-menu-hover")
                }
              >
                <Icon
                  className="size-4 shrink-0"
                  strokeWidth={1.75}
                  aria-hidden="true"
                />
                <span className="flex-1 truncate">{entry.label}</span>
                <span className="text-xs tabular-nums text-chrome-muted">
                  {count}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
