import { useId } from "react";

import type { ThemePreference } from "@/lib/settings";
import { cn } from "@/lib/utils";

/** The appearances in the order the page offers them. */
export const THEME_OPTIONS: { value: ThemePreference; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "cyberpunk", label: "Cyberpunk" },
  { value: "volt", label: "Volt" },
  { value: "mica", label: "Mica" },
];

/**
 * The appearance picker: one miniature window per theme, drawn in that theme's
 * surface, lines and accent, so the choice can be seen before it is made.
 *
 * A fieldset of radios, so the group is announced as one choice and the arrow
 * keys move between the cards.
 */
export function ThemeChoice({
  value,
  onChange,
}: {
  value: ThemePreference;
  onChange: (value: ThemePreference) => void;
}) {
  const name = useId();

  return (
    <fieldset className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      {THEME_OPTIONS.map((option) => {
        const selected = value === option.value;

        return (
          <label
            key={option.value}
            className="group min-w-0 cursor-pointer content-start"
          >
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={selected}
              onChange={() => onChange(option.value)}
              className="sr-only"
            />
            <span
              data-theme-swatch={option.value}
              className={cn(
                "flex h-16 flex-col gap-1.5 overflow-hidden rounded-chrome border p-2 transition-colors",
                "group-focus-within:ring-2 group-focus-within:ring-chrome-focus",
                selected
                  ? "border-chrome-focus"
                  : "border-chrome-line group-hover:border-chrome-muted",
              )}
            >
              <span className="flex items-center gap-1">
                <span className="size-1 rounded-full bg-swatch-line" />
                <span className="size-1 rounded-full bg-swatch-line" />
                <span className="size-1 rounded-full bg-swatch-line" />
                <span className="ml-auto h-1 w-8 rounded-full bg-swatch-line" />
              </span>
              <span className="mt-auto flex items-center gap-1.5">
                <span className="h-1 flex-1 rounded-full bg-swatch-line" />
                <span className="h-2.5 w-9 rounded-sm bg-swatch-accent" />
              </span>
              <span className="flex items-center gap-1.5">
                <span className="h-1 w-1/3 rounded-full bg-swatch-line" />
                <span className="h-1 w-1/4 rounded-full bg-swatch-line" />
              </span>
            </span>
            <span
              className={cn(
                "mt-1.5 block truncate px-0.5 text-xs transition-colors",
                selected
                  ? "text-chrome-title-text"
                  : "text-chrome-muted group-hover:text-chrome-title-text",
              )}
            >
              {option.label}
            </span>
          </label>
        );
      })}
    </fieldset>
  );
}
