import { useRef, useState, type KeyboardEvent } from "react";

import { CategoryFolderIcon, ViewIcon } from "@/components/icons/desktop-icons";
import { DisclosureGlyph, PlusGlyph } from "@/components/icons/ui-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { CategoryContextMenu } from "@/components/workspace/category-context-menu";
import {
  CategoryEditorDialog,
  RemoveCategoryDialog,
} from "@/components/workspace/category-dialogs";
import type { CategoryIcon } from "@/lib/category-icons";
import { OTHER_CATEGORY } from "@/lib/categories";
import type { CategoryName, DownloadFilter } from "@/lib/downloads";
import { cn } from "@/lib/utils";
import { useDownloads } from "@/state/download-store";

const VIEWS = [
  { id: "all", label: "All Downloads" },
  { id: "unfinished", label: "Unfinished" },
  { id: "finished", label: "Finished" },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

type Row =
  | {
      kind: "view";
      key: string;
      id: ViewId;
      label: string;
      count: number;
    }
  | {
      kind: "category";
      key: string;
      id: CategoryName;
      label: string;
      count: number;
      expanded: boolean;
      hasFolders: boolean;
      icon: CategoryIcon;
    }
  | {
      kind: "folder";
      key: string;
      id: CategoryName;
      path: string;
      label: string;
      count: number;
      icon: CategoryIcon;
    };

/**
 * The categories tree: the three download views over the categories, each
 * category unfolding into the save folders its downloads actually live in.
 * Rows are real tree rows, so the keyboard walks them with the arrows the way
 * a file manager does.
 */
export function CategoriesPane() {
  const {
    categories,
    filter,
    filterPath,
    setFilter,
    setFolder,
    counts,
    folders,
  } = useDownloads();
  // Only categories the user opened or closed explicitly are recorded; the
  // rest follow their content, so a category fills out as files arrive.
  const [openCategories, setOpenCategories] = useState<Record<string, boolean>>(
    {},
  );
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const [menu, setMenu] = useState<{
    filter: DownloadFilter;
    isFolder: boolean;
    at: { x: number; y: number };
  } | null>(null);
  // "new" opens the editor on an empty category; a name opens it on that one.
  const [editing, setEditing] = useState<CategoryName | "new" | null>(null);
  const [removing, setRemoving] = useState<CategoryName | null>(null);
  const itemRefs = useRef(new Map<string, HTMLButtonElement | null>());

  const viewRows: Row[] = VIEWS.map((view) => ({
    kind: "view",
    key: `view:${view.id}`,
    id: view.id,
    label: view.label,
    count: counts[view.id] ?? 0,
  }));

  const treeRows: Row[] = categories.flatMap<Row>((category) => {
    const children = folders[category.name] ?? [];
    const expanded = openCategories[category.name] ?? children.length > 0;
    const categoryRow: Row = {
      kind: "category",
      key: `category:${category.name}`,
      id: category.name,
      label: category.name,
      count: counts[category.name] ?? 0,
      expanded,
      hasFolders: children.length > 0,
      icon: category.icon,
    };
    if (!expanded) {
      return [categoryRow];
    }
    return [
      categoryRow,
      ...children.map<Row>((child) => ({
        kind: "folder",
        key: `folder:${category.name}:${child.path}`,
        id: category.name,
        path: child.path,
        label: child.label,
        count: child.count,
        icon: category.icon,
      })),
    ];
  });

  const rows = [...viewRows, ...treeRows];

  function isSelected(row: Row): boolean {
    if (row.kind === "folder") {
      return filter === row.id && filterPath === row.path;
    }
    return filter === row.id && filterPath === null;
  }

  function toggleCategory(category: CategoryName) {
    const children = folders[category] ?? [];
    const expanded = openCategories[category] ?? children.length > 0;
    // Collapsing a category must not leave its folder filtered behind hidden rows.
    if (expanded && filter === category && filterPath !== null) {
      setFilter(category);
    }
    setOpenCategories((current) => ({ ...current, [category]: !expanded }));
  }

  function focusRow(key: string) {
    setFocusKey(key);
    itemRefs.current.get(key)?.focus();
  }

  function openMenu(row: Row, at: { x: number; y: number }) {
    setMenu({ filter: row.id, isFolder: row.kind !== "view", at });
  }

  function handleKeyDown(
    event: KeyboardEvent<HTMLButtonElement>,
    row: Row,
  ) {
    const index = rows.findIndex((candidate) => candidate.key === row.key);
    const step = (delta: number) =>
      rows[(index + delta + rows.length) % rows.length];

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focusRow(step(1).key);
        break;
      case "ArrowUp":
        event.preventDefault();
        focusRow(step(-1).key);
        break;
      case "Home":
        event.preventDefault();
        focusRow(rows[0].key);
        break;
      case "End":
        event.preventDefault();
        focusRow(rows[rows.length - 1].key);
        break;
      case "ArrowRight": {
        if (row.kind !== "category") {
          break;
        }
        event.preventDefault();
        if (!row.expanded && row.hasFolders) {
          toggleCategory(row.id);
        } else if (row.expanded) {
          const child = rows[index + 1];
          if (child?.kind === "folder") {
            focusRow(child.key);
          }
        }
        break;
      }
      case "ArrowLeft": {
        event.preventDefault();
        if (row.kind === "folder") {
          focusRow(`category:${row.id}`);
        } else if (row.kind === "category" && row.expanded) {
          toggleCategory(row.id);
        }
        break;
      }
      case "ContextMenu":
        event.preventDefault();
        openMenu(row, menuSpot(event.currentTarget));
        break;
    }
  }

  const selectedRow = rows.find(isSelected);
  const editingCategory =
    editing && editing !== "new"
      ? (categories.find((category) => category.name === editing) ?? null)
      : null;
  const editorOpen = editing === "new" || editingCategory !== null;
  const removingCategory =
    categories.find((category) => category.name === removing) ?? null;
  const focusedKey = rows.some((row) => row.key === focusKey) ? focusKey : null;
  const rovingKey = focusedKey ?? selectedRow?.key ?? rows[0]?.key;

  function renderRow(row: Row) {
    const selected = isSelected(row);
    const depth = row.kind === "folder" ? 1 : 0;
    // A folder's path is longer than its row, so it gets the tooltip.
    const hint = row.kind === "folder" ? row.path : null;

    return (
      <li key={row.key}>
        <Tooltip content={hint} side="bottom" className="flex w-full">
          <button
            ref={(node) => {
              if (node) {
                itemRefs.current.set(row.key, node);
              } else {
                itemRefs.current.delete(row.key);
              }
            }}
            type="button"
            aria-current={selected ? "true" : undefined}
            aria-expanded={
              row.kind === "category" && row.hasFolders
                ? row.expanded
                : undefined
            }
            tabIndex={row.key === rovingKey ? 0 : -1}
            onClick={() => {
              setFocusKey(row.key);
              if (row.kind === "folder") {
                setFolder(row.id, row.path);
              } else {
                setFilter(row.id);
                if (row.kind === "category") {
                  toggleCategory(row.id);
                }
              }
            }}
            onContextMenu={(event) => {
              event.preventDefault();
              openMenu(row, { x: event.clientX, y: event.clientY });
            }}
            onFocus={() => setFocusKey(row.key)}
            onKeyDown={(event) => handleKeyDown(event, row)}
            className={cn(
              "group relative flex h-8 w-full items-center gap-1.5 rounded-chrome pr-2 text-left text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
              depth === 0 ? "pl-2" : "pl-9",
              selected
                ? "bg-chrome-menu-active text-chrome-title-text"
                : "text-chrome-menu-text hover:bg-chrome-menu-hover active:bg-chrome-toolbar-active",
            )}
          >
            <span className="grid size-4 shrink-0 place-items-center text-chrome-muted">
              {row.kind === "category" && row.hasFolders ? (
                <DisclosureGlyph
                  className={cn(
                    "size-3.5 transition-transform duration-150 ease-out motion-reduce:transition-none",
                    row.expanded && "rotate-90",
                  )}
                />
              ) : null}
            </span>

            {row.kind === "view" ? (
              <ViewIcon view={row.id} className="size-5 shrink-0" />
            ) : (
              <CategoryFolderIcon
                category={row.id}
                icon={row.icon}
                open={row.kind === "category" && row.expanded}
                className="size-5 shrink-0"
              />
            )}

            <span className="min-w-0 flex-1 truncate">{row.label}</span>
            <span
              className={cn(
                "shrink-0 text-chrome-micro tabular-nums",
                selected
                  ? "text-chrome-title-text"
                  : "text-chrome-muted group-hover:text-chrome-menu-text",
              )}
            >
              {row.count}
            </span>
          </button>
        </Tooltip>
      </li>
    );
  }

  return (
    <nav
      aria-label="Categories"
      className="flex w-48 shrink-0 flex-col overflow-hidden rounded-lg bg-chrome-panel sm:w-56 lg:w-64"
    >
      <div className="flex h-9 shrink-0 items-center justify-between pl-2.5 pr-1.5">
        <span className="flex min-w-0 items-center gap-1.5 text-chrome-micro font-medium tracking-wide text-chrome-muted">
          <CategoryFolderIcon
            category="General"
            open
            className="size-4 shrink-0"
          />
          <span className="truncate">Categories</span>
        </span>
        <Tooltip content="Add category" side="bottom">
          <button
            type="button"
            aria-label="Add category"
            onClick={() => setEditing("new")}
            className="grid size-6 shrink-0 place-items-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <PlusGlyph className="size-3.5" />
          </button>
        </Tooltip>
      </div>

      <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto px-1.5 py-2">
        <SectionLabel>Downloads</SectionLabel>
        <ul className="flex flex-col gap-px">{viewRows.map(renderRow)}</ul>

        <SectionLabel className="mt-3">Categories</SectionLabel>
        <ul className="flex flex-col gap-px">{treeRows.map(renderRow)}</ul>
      </div>

      {menu ? (
        <CategoryContextMenu
          filter={menu.filter}
          isFolder={menu.isFolder}
          removable={menu.filter !== OTHER_CATEGORY}
          at={menu.at}
          onAddCategory={() => setEditing("new")}
          onEditCategory={(name) => setEditing(name)}
          onRemoveCategory={(name) => setRemoving(name)}
          onClose={() => setMenu(null)}
        />
      ) : null}

      {editorOpen ? (
        <CategoryEditorDialog
          open
          category={editingCategory}
          onClose={() => setEditing(null)}
        />
      ) : null}

      {removingCategory ? (
        <RemoveCategoryDialog
          open
          category={removingCategory}
          onClose={() => setRemoving(null)}
        />
      ) : null}
    </nav>
  );
}

function SectionLabel({
  children,
  className,
}: {
  children: string;
  className?: string;
}) {
  return (
    <p
      className={cn(
        "flex items-center gap-2 px-2 pb-1.5 text-chrome-caption font-medium text-chrome-muted",
        className,
      )}
    >
      {children}
    </p>
  );
}

/** Where the menu of a category opens when the keyboard asked for it. */
function menuSpot(element: HTMLElement): { x: number; y: number } {
  const box = element.getBoundingClientRect();
  return { x: box.left, y: box.bottom };
}
