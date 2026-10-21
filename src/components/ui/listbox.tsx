import { useEffect, useId, useRef, useState } from "react";
import { ChevronDown, Check } from "lucide-react";

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
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const typeahead = useRef({ buffer: "", at: 0 });
  const baseId = useId();

  const selected = options.find((option) => option.value === value);

  useEffect(() => {
    if (open) {
      optionRefs.current[active]?.focus();
    }
  }, [open, active]);

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
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setOpen(false);
        }
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
        className="flex w-full items-center justify-between gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2 py-1.5 text-left text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
      >
        <span className="truncate">{selected?.label ?? label}</span>
        <ChevronDown className="size-3.5 shrink-0 text-chrome-muted" aria-hidden="true" />
      </button>

      {open ? (
        <ul
          id={`${baseId}-list`}
          role="listbox"
          aria-label={label}
          onKeyDown={handleKeyDown}
          className="absolute z-10 mt-1 max-h-56 w-full overflow-auto rounded-chrome border border-chrome-line bg-chrome-popup py-1 shadow-lg"
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
                  className="flex w-full items-center gap-2 px-2 py-1 text-left text-xs text-chrome-menu-text hover:bg-chrome-popup-hover focus-visible:bg-chrome-popup-hover focus-visible:outline-none"
                >
                  <Check
                    className={`size-3.5 shrink-0 ${isSelected ? "opacity-100" : "opacity-0"}`}
                    aria-hidden="true"
                  />
                  <span className="truncate">{option.label}</span>
                </button>
              </li>
            );
          })}
        </ul>
      ) : null}
    </div>
  );
}
