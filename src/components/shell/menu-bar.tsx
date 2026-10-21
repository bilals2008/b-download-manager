import { useEffect, useId, useRef, useState } from "react";
import { ChevronRight } from "lucide-react";

import type { ShellCommand } from "./commands";
import { MENUS, type MenuItem, type MenuRow } from "./menus";

interface MenuBarProps {
  onCommand: (command: ShellCommand) => void;
  /** Menu items whose text depends on the state of the app. */
  menuLabels?: Partial<Record<ShellCommand, string>>;
}

type FocusTarget = "first" | "last" | null;

/**
 * Windows-style menu bar: click opens, hover slides between open menus, and the
 * whole bar is reachable with the keyboard (arrows, Home/End, Escape, Alt+key).
 */
export function MenuBar({ onCommand, menuLabels }: MenuBarProps) {
  const [openIndex, setOpenIndex] = useState<number | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const barRef = useRef<HTMLElement>(null);
  const triggerRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const focusTargetRef = useRef<FocusTarget>(null);
  const baseId = useId();

  useEffect(() => {
    if (openIndex === null) {
      return;
    }

    function closeOnOutsidePointer(event: PointerEvent) {
      if (!barRef.current?.contains(event.target as Node)) {
        setOpenIndex(null);
      }
    }

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }
      const index = openIndex;
      setOpenIndex(null);
      if (index !== null) {
        triggerRefs.current[index]?.focus();
      }
    }

    document.addEventListener("pointerdown", closeOnOutsidePointer);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [openIndex]);

  useEffect(() => {
    function openFromAccessKey(event: KeyboardEvent) {
      if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }
      const index = MENUS.findIndex(
        (menu) => menu.accessKey === event.key.toLowerCase(),
      );
      if (index === -1) {
        return;
      }
      event.preventDefault();
      focusTargetRef.current = "first";
      setActiveIndex(index);
      setOpenIndex(index);
      triggerRefs.current[index]?.focus();
    }

    window.addEventListener("keydown", openFromAccessKey);
    return () => window.removeEventListener("keydown", openFromAccessKey);
  }, []);

  function focusTrigger(index: number) {
    const next = (index + MENUS.length) % MENUS.length;
    setActiveIndex(next);
    triggerRefs.current[next]?.focus();
  }

  function requestOpen(index: number, focus: FocusTarget) {
    focusTargetRef.current = focus;
    setActiveIndex(index);
    setOpenIndex(index);
  }

  function choose(item: MenuItem) {
    const index = openIndex;
    onCommand(item.command);
    setOpenIndex(null);
    if (index !== null) {
      triggerRefs.current[index]?.focus();
    }
  }

  function handleTriggerKeyDown(
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number,
  ) {
    switch (event.key) {
      case "ArrowDown":
      case "Enter":
      case " ":
        event.preventDefault();
        requestOpen(index, "first");
        break;
      case "ArrowUp":
        event.preventDefault();
        requestOpen(index, "last");
        break;
      case "ArrowLeft":
        event.preventDefault();
        focusTrigger(index - 1);
        break;
      case "ArrowRight":
        event.preventDefault();
        focusTrigger(index + 1);
        break;
      case "Home":
        event.preventDefault();
        focusTrigger(0);
        break;
      case "End":
        event.preventDefault();
        focusTrigger(MENUS.length - 1);
        break;
      case "Escape":
        if (openIndex !== null) {
          setOpenIndex(null);
        }
        break;
    }
  }

  const focusTarget = focusTargetRef.current;

  return (
    <nav
      ref={barRef}
      aria-label="Main menu"
      className="relative z-40 h-6 bg-chrome-menu"
    >
      <ul role="menubar" className="flex h-full items-stretch px-1">
        {MENUS.map((menu, index) => {
          const isOpen = openIndex === index;
          return (
            <li key={menu.id} role="none" className="relative">
              <button
                ref={(node) => {
                  triggerRefs.current[index] = node;
                }}
                type="button"
                role="menuitem"
                aria-haspopup="menu"
                aria-expanded={isOpen}
                aria-controls={isOpen ? `${baseId}-${menu.id}` : undefined}
                aria-keyshortcuts={`Alt+${menu.accessKey.toUpperCase()}`}
                tabIndex={activeIndex === index ? 0 : -1}
                onClick={() => {
                  focusTargetRef.current = null;
                  setActiveIndex(index);
                  setOpenIndex(isOpen ? null : index);
                }}
                onFocus={() => setActiveIndex(index)}
                onPointerEnter={() => {
                  if (openIndex !== null && openIndex !== index) {
                    focusTargetRef.current = null;
                    setOpenIndex(index);
                  }
                }}
                onKeyDown={(event) => handleTriggerKeyDown(event, index)}
                className={
                  "flex h-full items-center rounded-chrome px-2 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset " +
                  (isOpen ? "bg-chrome-menu-active" : "")
                }
              >
                {menu.label}
              </button>

              {isOpen ? (
                <MenuPopup
                  id={`${baseId}-${menu.id}`}
                  label={menu.label}
                  rows={menu.rows}
                  level={0}
                  focus={focusTarget}
                  menuLabels={menuLabels}
                  onChoose={choose}
                  onClose={() => {
                    setOpenIndex(null);
                    triggerRefs.current[index]?.focus();
                  }}
                />
              ) : null}
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

interface MenuPopupProps {
  id: string;
  label: string;
  rows: MenuRow[];
  level: number;
  focus: FocusTarget;
  menuLabels?: Partial<Record<ShellCommand, string>>;
  onChoose: (item: MenuItem) => void;
  onClose: () => void;
  onBack?: () => void;
}

/**
 * A dropdown, or a submenu of one. Submenus open on hover and on ArrowRight,
 * and ArrowLeft walks back out to the item that owns them.
 */
function MenuPopup({
  id,
  label,
  rows,
  level,
  focus,
  menuLabels,
  onChoose,
  onClose,
  onBack,
}: MenuPopupProps) {
  const [openSub, setOpenSub] = useState<number | null>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    const enabled = enabledRows(rows);
    if (enabled.length === 0) {
      return;
    }
    const target = focus === "last" ? enabled[enabled.length - 1] : enabled[0];
    itemRefs.current[target]?.focus();
    // The popup is mounted fresh every time it opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function moveFocus(from: number, delta: number) {
    const enabled = enabledRows(rows);
    if (enabled.length === 0) {
      return;
    }
    const position = enabled.indexOf(from);
    const next = enabled[(position + delta + enabled.length) % enabled.length];
    itemRefs.current[next]?.focus();
  }

  function handleItemKeyDown(
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number,
    item: MenuItem,
  ) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveFocus(index, 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(index, -1);
        break;
      case "Home": {
        event.preventDefault();
        const enabled = enabledRows(rows);
        itemRefs.current[enabled[0]]?.focus();
        break;
      }
      case "End": {
        event.preventDefault();
        const enabled = enabledRows(rows);
        itemRefs.current[enabled[enabled.length - 1]]?.focus();
        break;
      }
      case "ArrowRight":
        if (item.submenu) {
          event.preventDefault();
          setOpenSub(index);
        }
        break;
      case "ArrowLeft":
        if (onBack) {
          event.preventDefault();
          onBack();
        }
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        if (onBack) {
          onBack();
        } else {
          onClose();
        }
        break;
    }
  }

  return (
    <ul
      id={id}
      role="menu"
      aria-label={label}
      className={
        "absolute z-50 rounded-chrome border border-chrome-line bg-chrome-popup py-1 shadow-lg " +
        (level === 0
          ? "top-full left-0 mt-0.5 min-w-60"
          : "top-0 left-full ml-0.5 min-w-56")
      }
    >
      {rows.map((row, index) =>
        row === "separator" ? (
          <li
            key={`separator-${index}`}
            role="separator"
            aria-orientation="horizontal"
            className="my-1 h-px bg-chrome-line"
          />
        ) : (
          <li key={row.command + index} role="none" className="relative">
            <button
              ref={(node) => {
                itemRefs.current[index] = node;
              }}
              type="button"
              role="menuitem"
              tabIndex={-1}
              aria-disabled={row.disabled || undefined}
              aria-haspopup={row.submenu ? "menu" : undefined}
              aria-expanded={row.submenu ? openSub === index : undefined}
              onClick={() => {
                if (row.disabled) {
                  return;
                }
                if (row.submenu) {
                  setOpenSub(index);
                  return;
                }
                onChoose(row);
              }}
              onPointerEnter={() => setOpenSub(row.submenu ? index : null)}
              onKeyDown={(event) => handleItemKeyDown(event, index, row)}
              className={
                "flex w-full items-center gap-6 px-3 py-1.5 text-left text-xs focus-visible:bg-chrome-popup-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
                (row.disabled
                  ? "cursor-default text-chrome-muted/60"
                  : "text-chrome-menu-text hover:bg-chrome-popup-hover")
              }
            >
              <span className="truncate">
                {menuLabels?.[row.command] ?? row.label}
              </span>
              {row.submenu ? (
                <ChevronRight
                  className="ml-auto size-3.5 shrink-0 text-chrome-muted"
                  aria-hidden="true"
                />
              ) : row.shortcut ? (
                <span
                  aria-hidden="true"
                  className="ml-auto shrink-0 text-xs text-chrome-muted"
                >
                  {row.shortcut}
                </span>
              ) : null}
            </button>

            {row.submenu && openSub === index ? (
              <MenuPopup
                id={`${id}-${index}`}
                label={row.label}
                rows={row.submenu}
                level={level + 1}
                focus="first"
                menuLabels={menuLabels}
                onChoose={onChoose}
                onClose={onClose}
                onBack={() => {
                  setOpenSub(null);
                  itemRefs.current[index]?.focus();
                }}
              />
            ) : null}
          </li>
        ),
      )}
    </ul>
  );
}

/** Indexes of the rows a keyboard user can land on. */
function enabledRows(rows: MenuRow[]): number[] {
  return rows
    .map((row, index) => (row === "separator" || row.disabled ? -1 : index))
    .filter((index) => index >= 0);
}
