import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { CheckGlyph, ChevronDown } from "@/components/icons/ui-icons";

export interface ListBoxOption {
  value: string;
  label: string;
}

interface ListBoxProps {
  label: string;
  options: ListBoxOption[];
  value: string;
  onChange: (value: string) => void;
}

/** Where the popup hangs, in viewport coordinates. */
interface Placement {
  left: number;
  width: number;
  maxHeight: number;
  top?: number;
  bottom?: number;
}

function samePlace(a: Placement | null, b: Placement): boolean {
  return (
    a !== null &&
    a.left === b.left &&
    a.width === b.width &&
    a.maxHeight === b.maxHeight &&
    a.top === b.top &&
    a.bottom === b.bottom
  );
}

/**
 * A combo box made of a button and an ARIA listbox. Native `select` elements
 * draw their own chevron and cannot follow the theme, so the trigger, the
 * popup and the keyboard behaviour are built here instead.
 */
export function ListBox({ label, options, value, onChange }: ListBoxProps) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(() =>
    Math.max(
      0,
      options.findIndex((option) => option.value === value),
    ),
  );
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const typeahead = useRef({ buffer: "", at: 0 });
  const baseId = useId();
  const [placement, setPlacement] = useState<Placement | null>(null);

  const selected = options.find((option) => option.value === value);

  // The popup is painted in the dialog itself, fixed to the viewport, so the
  // rounded corners and the scroll area of a dialog cannot clip it. It opens
  // downwards unless the trigger sits low and there is more room above.
  useLayoutEffect(() => {
    if (!open) {
      setPlacement(null);
      return;
    }
    const update = () => {
      const trigger = triggerRef.current;
      if (!trigger) {
        return;
      }
      const rect = trigger.getBoundingClientRect();
      const gap = 4;
      const below = window.innerHeight - rect.bottom - gap;
      const above = rect.top - gap;
      const flip = below < 160 && above > below;
      const next: Placement = {
        left: rect.left,
        width: rect.width,
        maxHeight: Math.min(224, Math.max(80, flip ? above : below)),
        ...(flip
          ? { bottom: window.innerHeight - rect.top + gap }
          : { top: rect.bottom + gap }),
      };
      setPlacement((current) => (samePlace(current, next) ? current : next));
    };
    // Scrolling the list itself must not move it.
    const onScroll = (event: Event) => {
      const target = event.target;
      if (target instanceof Node && listRef.current?.contains(target)) {
        return;
      }
      update();
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  useEffect(() => {
    if (open && placement) {
      optionRefs.current[active]?.focus();
    }
  }, [open, active, placement]);

  function close(returnFocus = true) {
    setOpen(false);
    if (returnFocus) {
      triggerRef.current?.focus();
    }
  }

  function commit(index: number) {
    const option = options[index];
    if (option) {
      onChange(option.value);
    }
    close();
  }

  function move(delta: number) {
    setActive((current) => (current + delta + options.length) % options.length);
  }

  function typeaheadMatch(key: string) {
    const now = Date.now();
    const state = typeahead.current;
    state.buffer = now - state.at > 800 ? key : state.buffer + key;
    state.at = now;
    const match = options.findIndex((option) =>
      option.label.toLowerCase().startsWith(state.buffer.toLowerCase()),
    );
    if (match >= 0) {
      setActive(match);
    }
  }

  function handleKeyDown(event: React.KeyboardEvent) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (!open) {
          setOpen(true);
        } else {
          move(1);
        }
        break;
      case "ArrowUp":
        event.preventDefault();
        if (!open) {
          setOpen(true);
        } else {
          move(-1);
        }
        break;
      case "Home":
        if (open) {
          event.preventDefault();
          setActive(0);
        }
        break;
      case "End":
        if (open) {
          event.preventDefault();
          setActive(options.length - 1);
        }
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        if (open) {
          commit(active);
        } else {
          setOpen(true);
        }
        break;
      case "Escape":
        if (open) {
          event.preventDefault();
          close();
        }
        break;
      case "Tab":
        close(false);
        break;
      default:
        if (open && event.key.length === 1) {
          typeaheadMatch(event.key);
        }
    }
  }

  return (
    <div
      className="relative"
      onBlur={(event) => {
        const next = event.relatedTarget as Node | null;
        if (
          next &&
          (event.currentTarget.contains(next) || listRef.current?.contains(next))
        ) {
          return;
        }
        setOpen(false);
      }}
    >
      <button
        ref={triggerRef}
        type="button"
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={`${baseId}-list`}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={handleKeyDown}
        className="flex w-full items-center justify-between gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 py-1.5 text-left text-xs text-chrome-menu-text transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
      >
        <span className="truncate">{selected?.label ?? label}</span>
        <ChevronDown className="size-3.5 shrink-0 text-chrome-muted" />
      </button>

      {open && placement
        ? createPortal(
            <ul
              ref={listRef}
              id={`${baseId}-list`}
              role="listbox"
              aria-label={label}
              onKeyDown={handleKeyDown}
              style={{
                left: placement.left,
                width: placement.width,
                maxHeight: placement.maxHeight,
                top: placement.top,
                bottom: placement.bottom,
              }}
              className="fixed z-50 overflow-auto rounded-lg bg-chrome-popup p-1 shadow-float"
            >
              {options.map((option, index) => {
                const isSelected = option.value === value;
                return (
                  <li key={option.value} role="none">
                    <button
                      ref={(node) => {
                        optionRefs.current[index] = node;
                      }}
                      type="button"
                      role="option"
                      aria-selected={isSelected}
                      tabIndex={-1}
                      onClick={() => commit(index)}
                      onPointerEnter={() => setActive(index)}
                      className="flex w-full items-center gap-2 rounded-chrome px-2 py-1.5 text-left text-xs text-chrome-menu-text transition-colors hover:bg-chrome-popup-hover focus-visible:bg-chrome-popup-hover focus-visible:outline-none"
                    >
                      <CheckGlyph
                        className={`size-3.5 shrink-0 ${isSelected ? "opacity-100" : "opacity-0"}`}
                      />
                      <span className="truncate">{option.label}</span>
                    </button>
                  </li>
                );
              })}
            </ul>,
            // Inside a modal dialog everything outside it is inert, so the
            // popup goes into the dialog rather than into the body.
            triggerRef.current?.closest("dialog") ?? document.body,
          )
        : null}
    </div>
  );
}
