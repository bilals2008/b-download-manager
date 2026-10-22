import { useRef, useState, type ComponentType, type KeyboardEvent } from "react";
import {
  CircleCheck,
  CircleDashed,
  Inbox,
  PanelLeftClose,
  PanelLeftOpen,
  type LucideProps,
} from "lucide-react";

import { CategoryContextMenu } from "@/components/workspace/category-context-menu";
import {
  CATEGORIES,
  type DownloadFilter,
} from "@/lib/downloads";
import { cn } from "@/lib/utils";
import { useDownloads } from "@/state/download-store";

import { CATEGORY_ICONS } from "./category-icons";

type Icon = ComponentType<LucideProps>;

interface Entry {
  id: DownloadFilter;
  label: string;
  icon: Icon;
  tint?: string;
  /** True for a category folder, false for the three groups above them. */
  isFolder?: boolean;
}

const GROUPS: Entry[] = [
  { id: "all", label: "All Downloads", icon: Inbox },
  { id: "unfinished", label: "Unfinished", icon: CircleDashed },
  { id: "finished", label: "Finished", icon: CircleCheck },
];

const FOLDERS: Entry[] = CATEGORIES.map((category) => ({
  id: category,
  label: category,
  icon: CATEGORY_ICONS[category].icon,
  tint: CATEGORY_ICONS[category].tint,
  isFolder: true,
}));

const ENTRIES: Entry[] = [...GROUPS, ...FOLDERS];

/**
 * The categories pane: IDM's three groups over the download categories,
 * grouped like folders and collapsible to an icon rail. Every entry carries
 * the number of downloads behind it.
 */
export function CategoriesPane() {
  const { filter, setFilter, counts } = useDownloads();
  const [collapsed, setCollapsed] = useState(false);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  // The category whose menu is open, and where the pointer was.
  const [menu, setMenu] = useState<{
    filter: DownloadFilter;
    isFolder: boolean;
    at: { x: number; y: number };
  } | null>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);

  function openMenu(
    entry: Entry,
    at: { x: number; y: number },
  ) {
    setMenu({
      filter: entry.id,
      isFolder: entry.isFolder ?? false,
      at,
    });
  }

  function focusItem(index: number) {
    const next = (index + ENTRIES.length) % ENTRIES.length;
    setFocusIndex(next);
    itemRefs.current[next]?.focus();
  }

  function handleKeyDown(
    event: KeyboardEvent<HTMLButtonElement>,
    index: number,
  ) {
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
        focusItem(ENTRIES.length - 1);
        break;
      case "ContextMenu":
        event.preventDefault();
        openMenu(ENTRIES[index], menuSpot(event.currentTarget));
        break;
    }
  }

  const selectedIndex = Math.max(
    0,
    ENTRIES.findIndex((entry) => entry.id === filter),
  );
  const rovingIndex = focusIndex ?? selectedIndex;

  function renderEntry(entry: Entry, index: number) {
    return (
      <li key={entry.id}>
        <FilterItem
          entry={entry}
          count={counts[entry.id] ?? 0}
          selected={entry.id === filter}
          collapsed={collapsed}
          tabIndex={index === rovingIndex ? 0 : -1}
          buttonRef={(node) => {
            itemRefs.current[index] = node;
          }}
          onSelect={() => {
            setFocusIndex(index);
            setFilter(entry.id);
          }}
          onOpenMenu={(at) => openMenu(entry, at)}
          onFocus={() => setFocusIndex(index)}
          onKeyDown={(event) => handleKeyDown(event, index)}
        />
      </li>
    );
  }

  return (
    <nav
      aria-label="Categories"
      className={cn(
        "flex shrink-0 flex-col border-r border-chrome-line bg-chrome-panel",
        collapsed ? "w-12" : "w-44 sm:w-52 lg:w-60",
      )}
    >
      <div
        className={cn(
          "flex h-8 shrink-0 items-center border-b border-chrome-line/70",
          collapsed ? "justify-center px-1" : "justify-between pl-3 pr-1.5",
        )}
      >
        {collapsed ? null : (
          <span className="text-[11px] font-medium tracking-wide text-chrome-muted">
            Library
          </span>
        )}
        <button
          type="button"
          aria-label={collapsed ? "Expand categories" : "Collapse categories"}
          aria-expanded={!collapsed}
          title={collapsed ? "Expand categories" : "Collapse categories"}
          onClick={() => setCollapsed((value) => !value)}
          className="grid size-6 shrink-0 place-items-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          {collapsed ? (
            <PanelLeftOpen className="size-3.5" aria-hidden="true" />
          ) : (
            <PanelLeftClose className="size-3.5" aria-hidden="true" />
          )}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-1.5 py-2">
        {collapsed ? null : <SectionLabel>Downloads</SectionLabel>}
        <ul className="flex flex-col gap-0.5">{GROUPS.map(renderEntry)}</ul>

        {collapsed ? (
          <div className="my-2 border-t border-chrome-line/70" />
        ) : (
          <SectionLabel className="mt-3">Folders</SectionLabel>
        )}
        <ul className="flex flex-col gap-0.5">{FOLDERS.map((entry, index) => renderEntry(entry, index + GROUPS.length))}</ul>
      </div>

      {menu ? (
        <CategoryContextMenu
          filter={menu.filter}
          isFolder={menu.isFolder}
          at={menu.at}
          onClose={() => setMenu(null)}
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
        "px-2 pb-1 text-[10px] font-semibold tracking-wider text-chrome-muted uppercase",
        className,
      )}
    >
      {children}
    </p>
  );
}

interface FilterItemProps {
  entry: Entry;
  count: number;
  selected: boolean;
  collapsed: boolean;
  tabIndex: number;
  buttonRef: (node: HTMLButtonElement | null) => void;
  onSelect: () => void;
  /** Open the menu of the category, the way a right click does. */
  onOpenMenu: (at: { x: number; y: number }) => void;
  onFocus: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

function FilterItem({
  entry,
  count,
  selected,
  collapsed,
  tabIndex,
  buttonRef,
  onSelect,
  onOpenMenu,
  onFocus,
  onKeyDown,
}: FilterItemProps) {
  const Icon = entry.icon;

  return (
    <button
      ref={buttonRef}
      type="button"
      aria-current={selected ? "true" : undefined}
      title={collapsed ? `${entry.label} (${count})` : undefined}
      tabIndex={tabIndex}
      onClick={onSelect}
      onContextMenu={(event) => {
        event.preventDefault();
        onOpenMenu({ x: event.clientX, y: event.clientY });
      }}
      onFocus={onFocus}
      onKeyDown={onKeyDown}
      className={cn(
        "group flex w-full items-center gap-2 rounded-chrome px-1.5 py-1.5 text-left text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
        selected
          ? "bg-chrome-menu-active text-chrome-title-text"
          : "text-chrome-menu-text hover:bg-chrome-menu-hover active:bg-chrome-toolbar-active",
        collapsed && "justify-center px-0",
      )}
    >
      <span
        className={cn(
          "grid size-6 shrink-0 place-items-center rounded-chrome",
          selected
            ? "bg-chrome-title/60"
            : "bg-chrome-popup group-hover:bg-chrome-toolbar",
        )}
      >
        <Icon
          className={cn("size-3.5", entry.tint ?? "text-chrome-muted")}
          strokeWidth={1.75}
          aria-hidden="true"
        />
      </span>
      {collapsed ? null : (
        <>
          <span className="min-w-0 flex-1 truncate">{entry.label}</span>
          <span
            className={cn(
              "shrink-0 text-[11px] tabular-nums",
              selected
                ? "text-chrome-title-text"
                : "text-chrome-muted group-hover:text-chrome-menu-text",
            )}
          >
            {count}
          </span>
        </>
      )}
    </button>
  );
}

/** Where the menu of a category opens when the keyboard asked for it. */
function menuSpot(element: HTMLElement): { x: number; y: number } {
  const box = element.getBoundingClientRect();
  return { x: box.left, y: box.bottom };
}
