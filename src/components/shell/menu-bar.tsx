import { useEffect, useId, useRef, useState } from "react";

import type { ShellCommand } from "./commands";
import { MENUS } from "./menus";
import { MenuPopup, type FocusTarget, type MenuItem } from "./menu-popup";

interface MenuBarProps {
  onCommand: (command: ShellCommand) => void;
  /** Menu items whose text depends on the state of the app. */
  menuLabels?: Partial<Record<ShellCommand, string>>;
  /** Which radio rows are picked, the way IDM ticks the toolbar size. */
  menuChecks?: Partial<Record<ShellCommand, boolean>>;
}

/**
 * Windows-style menu bar: click opens, hover slides between open menus, and the
 * whole bar is reachable with the keyboard (arrows, Home/End, Escape, Alt+key).
 */
export function MenuBar({ onCommand, menuLabels, menuChecks }: MenuBarProps) {
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

  function choose(item: MenuItem<ShellCommand>) {
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
                  checked={menuChecks}
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
