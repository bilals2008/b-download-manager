import { CategoryFolderIcon } from "@/components/icons/desktop-icons";
import { Tooltip } from "@/components/ui/tooltip";
import {
  CATEGORY_GLYPHS,
  CATEGORY_GLYPH_IDS,
  CATEGORY_PALETTES,
  CATEGORY_PALETTE_IDS,
  DEFAULT_CUSTOM_COLOR,
  type CategoryIcon,
} from "@/lib/category-icons";
import { cn } from "@/lib/utils";

interface CategoryIconPickerProps {
  /** The category whose icon is being chosen, so the preview reads like the tree. */
  category: string;
  value: CategoryIcon;
  onChange: (icon: CategoryIcon) => void;
  disabled?: boolean;
}

/**
 * The icon of a category: sixteen drawings, six theme palettes and a colour of
 * the user's own. The drawing and the palette are ids, so the theme stays in
 * charge of the paint; a custom colour is the one value the user owns.
 */
export function CategoryIconPicker({
  category,
  value,
  onChange,
  disabled = false,
}: CategoryIconPickerProps) {
  const custom = Boolean(value.color);
  return (
    <div>
      <span className="mb-1.5 block text-xs text-chrome-muted">Icon</span>
      <div className="rounded-chrome border border-chrome-line bg-chrome-canvas p-1.5">
        <div
          role="group"
          aria-label="Icon shape"
          className="grid grid-cols-8 gap-1"
        >
          {CATEGORY_GLYPH_IDS.map((glyph) => {
            const label = CATEGORY_GLYPHS[glyph];
            const picked = value.glyph === glyph;
            return (
              <Tooltip key={glyph} content={label} side="top">
                <button
                  type="button"
                  aria-label={label}
                  aria-pressed={picked}
                  disabled={disabled}
                  onClick={() => onChange({ ...value, glyph })}
                  className={cn(
                    "grid size-7 place-items-center rounded-chrome transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-50",
                    picked
                      ? "bg-chrome-menu-active"
                      : "hover:bg-chrome-menu-hover",
                  )}
                >
                  <CategoryFolderIcon
                    category={category || "Other"}
                    icon={{ ...value, glyph }}
                    className="size-5"
                  />
                </button>
              </Tooltip>
            );
          })}
        </div>

        <div
          role="group"
          aria-label="Icon colour"
          className="mt-1.5 flex items-center gap-1 border-t border-chrome-line pt-1.5"
        >
          {CATEGORY_PALETTE_IDS.map((id) => {
            const palette = CATEGORY_PALETTES[id];
            const picked = !custom && value.palette === id;
            return (
              <Tooltip key={id} content={palette.label} side="top">
                <button
                  type="button"
                  aria-label={palette.label}
                  aria-pressed={picked}
                  disabled={disabled}
                  onClick={() =>
                    onChange({ ...value, palette: id, color: undefined })
                  }
                  className={cn(
                    "grid size-6 place-items-center rounded-chrome transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-50",
                    picked ? "bg-chrome-menu-active" : "hover:bg-chrome-menu-hover",
                  )}
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      "size-3.5 rounded-full",
                      picked && "ring-2 ring-chrome-focus ring-offset-1 ring-offset-chrome-canvas",
                    )}
                    style={{ backgroundColor: `var(${palette.base})` }}
                  />
                </button>
              </Tooltip>
            );
          })}

          <Tooltip content="Custom colour" side="top">
            <label
              className={cn(
                "relative grid size-6 cursor-pointer place-items-center rounded-chrome transition-colors focus-within:ring-2 focus-within:ring-chrome-focus focus-within:ring-inset",
                custom ? "bg-chrome-menu-active" : "hover:bg-chrome-menu-hover",
                disabled && "cursor-default opacity-50",
              )}
            >
              <span
                aria-hidden="true"
                className={cn(
                  "size-3.5 rounded-full",
                  custom &&
                    "ring-2 ring-chrome-focus ring-offset-1 ring-offset-chrome-canvas",
                )}
                style={{
                  background:
                    value.color ??
                    "conic-gradient(from 0deg, var(--icon-red), var(--icon-amber), var(--icon-green), var(--icon-blue), var(--icon-purple), var(--icon-red))",
                }}
              />
              <input
                type="color"
                aria-label="Custom colour"
                disabled={disabled}
                value={value.color ?? DEFAULT_CUSTOM_COLOR}
                onChange={(event) =>
                  onChange({ ...value, color: event.target.value })
                }
                className="absolute inset-0 size-full cursor-pointer opacity-0 disabled:cursor-default"
              />
            </label>
          </Tooltip>

          <span className="ml-auto pr-1 text-chrome-micro text-chrome-muted">
            {CATEGORY_GLYPHS[value.glyph]}
          </span>
        </div>
      </div>
    </div>
  );
}
