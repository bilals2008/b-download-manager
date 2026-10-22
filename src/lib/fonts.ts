import type { UiFont, UiWeight } from "@/lib/settings";

/**
 * The faces the appearance page offers. The stacks live in `src/index.css`,
 * one block per `data-font` value; nothing here knows a font file name.
 */
export interface FontOption {
  value: UiFont;
  label: string;
  /** One line on what the face is like, shown under the picker. */
  note: string;
}

export const FONT_OPTIONS: FontOption[] = [
  {
    value: "inter",
    label: "Inter",
    note: "Neutral screen face. The default.",
  },
  {
    value: "geist",
    label: "Geist",
    note: "Geometric, slightly tighter.",
  },
  {
    value: "dmsans",
    label: "DM Sans",
    note: "Round and friendly, large x-height.",
  },
  {
    value: "jakarta",
    label: "Plus Jakarta Sans",
    note: "Contemporary, slightly condensed.",
  },
  {
    value: "instrument",
    label: "Instrument Sans",
    note: "Humanist, a little warm.",
  },
  {
    value: "plex",
    label: "IBM Plex Sans",
    note: "Humanist, clearest figures.",
  },
  {
    value: "jetbrains",
    label: "JetBrains Mono",
    note: "Monospaced: columns line up, text runs wide.",
  },
  {
    value: "system",
    label: "System (Segoe UI)",
    note: "Segoe UI, the Windows face.",
  },
];

/** How heavy the text is, as whole steps up the scale. */
export const WEIGHT_OPTIONS: { value: UiWeight; label: string }[] = [
  { value: "regular", label: "Regular" },
  { value: "medium", label: "Medium" },
  { value: "semibold", label: "Semibold" },
];

/**
 * Appearance, the same way `theme.ts` does it: one attribute on the root picks
 * the block of `--app-font-ui` or `--app-font-weight` that every component
 * already reads. Set before the first render in `main.tsx`, so no window
 * starts in the wrong face.
 */
export function applyFont(font: UiFont): void {
  document.documentElement.dataset.font = font;
}

export function applyWeight(weight: UiWeight): void {
  document.documentElement.dataset.weight = weight;
}
