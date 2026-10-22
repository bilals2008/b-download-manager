import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import { CheckGlyph, ChevronRight } from "@/components/icons/ui-icons";

/** One command in a menu. */
export interface MenuItem<T extends string = string> {
  /** What the command does; the app maps it to an action. */
  command: T;
  label: string;
  /** A glyph that names the command, drawn on the left of the row. */
  icon?: ReactNode;
  shortcut?: string;
  /** Commands whose screen is not built yet stay visible but inert. */
  disabled?: boolean;
  /** A command that removes or overwrites data is drawn in the error tone. */
  danger?: boolean;
  /** A radio row: the menu shows which one is picked. */
  radio?: boolean;
  /** A nested menu, the way IDM groups "Export", "Arrange files" and friends. */
  submenu?: MenuRow<T>[];
}

/** `"separator"` draws the hairline rule between groups of commands. */
export type MenuRow<T extends string = string> = MenuItem<T> | "separator";

/** Where the keyboard focus lands when a menu opens. */
export type FocusTarget = "first" | "last" | null;

/** Why a menu closed, so the owner knows whether to take the focus back. */
export type MenuCloseReason = "pointer" | "keyboard";

export interface MenuPopupProps<T extends string> {
  id: string;
  label: string;
  rows: MenuRow<T>[];
  /** 0 for a menu of the bar, 1 for a submenu of one. */
  level: number;
  focus: FocusTarget;
  /** Menu items whose text depends on the state of the app. */
  menuLabels?: Partial<Record<T, string>>;
  /** Which radio rows are picked. */
  checked?: Partial<Record<T, boolean>>;
  /** Where a context menu opens. A menu of the bar is placed by its trigger. */
  at?: { x: number; y: number };
  onChoose: (item: MenuItem<T>) => void;
  onClose: (reason?: MenuCloseReason) => void;
  onBack?: () => void;
}

/**
 * A dropdown, a submenu of one, or a context menu.
 *
 * The menus of the bar hang under their trigger; a context menu is placed at
 * the pointer and turns back when it would leave the window. Submenus open on
 * hover and on ArrowRight, and ArrowLeft walks back out to the item that owns
 * them.
 */
export function MenuPopup<T extends string>({
  id,
  label,
  rows,
  level,
  focus,
  menuLabels,
  checked,
  at,
  onChoose,
  onClose,
  onBack,
}: MenuPopupProps<T>) {
  const [openSub, setOpenSub] = useState<number | null>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const listRef = useRef<HTMLUListElement>(null);
  const [spot, setSpot] = useState<{ x: number; y: number } | null>(at ?? null);

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

  // A context menu is measured after it opens, so it never hangs over the
  // edge of the window.
  useLayoutEffect(() => {
    if (!at) {
      return;
    }
    const list = listRef.current;
    if (!list) {
      return;
    }
    const box = list.getBoundingClientRect();
    const margin = 8;
    const x = Math.max(margin, Math.min(at.x, window.innerWidth - box.width - margin));
    const y = Math.max(
      margin,
      Math.min(at.y, window.innerHeight - box.height - margin),
    );
    setSpot((current) =>
      current && current.x === x && current.y === y ? current : { x, y },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [at?.x, at?.y]);

  // A click outside a context menu closes it, and so does anything that moves
  // the page under it: the pointer is the reason, so focus is left alone.
  useEffect(() => {
    if (!at) {
      return;
    }
    function closeOnOutsidePointer(event: PointerEvent) {
      if (!listRef.current?.contains(event.target as Node)) {
        onClose("pointer");
      }
    }
    function closeOnMove() {
      onClose("pointer");
    }
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    window.addEventListener("resize", closeOnMove);
    window.addEventListener("scroll", closeOnMove, true);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      window.removeEventListener("resize", closeOnMove);
      window.removeEventListener("scroll", closeOnMove, true);
    };
  }, [at, onClose]);

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
    item: MenuItem<T>,
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
          onClose("keyboard");
        }
        break;
    }
  }

  const placement = spot
    ? "fixed min-w-60"
    : level === 0
      ? "absolute top-full left-0 mt-0.5 min-w-60"
      : "absolute top-0 left-full ml-0.5 min-w-56";

  return (
    <ul
      id={id}
      ref={listRef}
      role="menu"
      aria-label={label}
      style={spot ? { left: spot.x, top: spot.y } : undefined}
      className={`z-50 rounded-lg bg-chrome-popup p-1 shadow-float ${placement}`}
    >
      {rows.map((row, index) =>
        row === "separator" ? (
          <li
            key={`separator-${index}`}
            role="separator"
            aria-orientation="horizontal"
            className="my-1 h-0"
          />
        ) : (
          <li key={row.command + index} role="none" className="relative">
            <button
              ref={(node) => {
                itemRefs.current[index] = node;
              }}
              type="button"
              role={row.radio ? "menuitemradio" : "menuitem"}
              aria-checked={row.radio ? (checked?.[row.command] ?? false) : undefined}
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
                "flex w-full items-center gap-2.5 rounded-chrome px-2.5 py-1.5 text-left text-xs transition-colors focus-visible:bg-chrome-popup-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
                (row.disabled
                  ? "cursor-default text-chrome-muted/60"
                  : row.danger
                    ? "text-chrome-error hover:bg-chrome-popup-hover"
                    : "text-chrome-menu-text hover:bg-chrome-popup-hover")
              }
            >
              {row.icon ? (
                <span className="grid size-4 shrink-0 place-items-center">
                  {row.icon}
                </span>
              ) : null}
              <span className="truncate">
                {menuLabels?.[row.command] ?? row.label}
              </span>
              {row.submenu ? (
                <ChevronRight className="ml-auto size-3.5 shrink-0 text-chrome-muted" />
              ) : row.radio && checked?.[row.command] ? (
                <CheckGlyph className="ml-auto size-3.5 shrink-0 text-chrome-focus" />
              ) : row.shortcut ? (
                <span
                  aria-hidden="true"
                  className="ml-auto shrink-0 pl-4 text-xs text-chrome-muted"
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
                checked={checked}
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
function enabledRows<T extends string>(rows: MenuRow<T>[]): number[] {
  return rows
    .map((row, index) => (row === "separator" || row.disabled ? -1 : index))
    .filter((index) => index >= 0);
}
