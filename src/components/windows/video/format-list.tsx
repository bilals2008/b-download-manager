import { useEffect, useRef, useState } from "react";

import { CheckGlyph } from "@/components/icons/ui-icons";
import { formatSize } from "@/lib/downloads";
import type { QualityRow } from "@/lib/media";

interface FormatListProps {
  rows: QualityRow[];
  value: string | null;
  onSelect: (id: string) => void;
  /** Enter starts the selected row; the button in the footer does the same. */
  onActivate: (id: string) => void;
  disabled?: boolean;
  autoFocus?: boolean;
}

/**
 * The quality rows, as one ARIA listbox.
 *
 * A native `<select>` cannot show a size beside a name or a note under it, and
 * it draws its own popup outside the theme. This is the listbox pattern the
 * dialog lists use, with roving focus: arrows move, Enter starts, Space only
 * selects, Home and End jump.
 */
export function FormatList({
  rows,
  value,
  onSelect,
  onActivate,
  disabled = false,
  autoFocus = false,
}: FormatListProps) {
  const [active, setActive] = useState(0);
  const buttons = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    const index = rows.findIndex((row) => row.id === value);
    setActive(index >= 0 ? index : 0);
  }, [rows, value]);

  useEffect(() => {
    if (autoFocus && rows.length) {
      buttons.current[Math.min(active, rows.length - 1)]?.focus();
    }
    // Only the first ready render takes the focus; later polls must not steal
    // it from a reader who moved on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoFocus, rows.length]);

  function move(next: number) {
    if (!rows.length) {
      return;
    }
    const index = (next + rows.length) % rows.length;
    setActive(index);
    onSelect(rows[index].id);
    buttons.current[index]?.focus();
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (!rows.length) {
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        move(active + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(active - 1);
        break;
      case "Home":
        event.preventDefault();
        move(0);
        break;
      case "End":
        event.preventDefault();
        move(rows.length - 1);
        break;
      case "Enter":
        event.preventDefault();
        onActivate(rows[active].id);
        break;
      case " ":
        event.preventDefault();
        onSelect(rows[active].id);
        break;
      default:
        break;
    }
  }

  return (
    <div
      role="listbox"
      aria-label="Quality"
      aria-activedescendant={rows.length ? `quality-${active}` : undefined}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto"
    >
      {rows.map((row, index) => {
        const selected = row.id === value;
        const size = row.size === null ? null : formatSize(row.size);
        return (
          <button
            key={`${row.kind}-${row.id}`}
            id={`quality-${index}`}
            ref={(node) => {
              buttons.current[index] = node;
            }}
            type="button"
            role="option"
            aria-selected={selected}
            aria-label={`${row.label}${row.note ? `, ${row.note}` : ""}${size ? `, ${size}` : ""}`}
            tabIndex={index === active ? 0 : -1}
            disabled={disabled}
            onClick={() => {
              setActive(index);
              onSelect(row.id);
            }}
            onFocus={() => setActive(index)}
            className={`flex w-full items-center gap-2 rounded-chrome px-2.5 py-2 text-left transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45 ${
              selected
                ? "bg-chrome-menu-active text-chrome-menu-text"
                : "text-chrome-menu-text hover:bg-chrome-popup-hover"
            }`}
          >
            <CheckGlyph
              className={`size-3.5 shrink-0 ${selected ? "opacity-100" : "opacity-0"}`}
            />
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span className="truncate text-xs">{row.label}</span>
              {row.note ? (
                <span className="truncate text-chrome-micro text-chrome-muted">
                  {row.note}
                </span>
              ) : null}
            </span>
            {size ? (
              <span className="shrink-0 text-chrome-micro tabular-nums text-chrome-muted">
                {size}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
