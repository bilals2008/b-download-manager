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
    note: "Clean and even.",
  },
  {
    value: "googlesans",
    label: "Google Sans",
    note: "Google's own font. The default.",
  },
  {
    value: "geist",
    label: "Geist",
    note: "Modern and close together.",
  },
  {
    value: "dmsans",
    label: "DM Sans",
    note: "Round and friendly.",
  },
  {
    value: "jakarta",
    label: "Plus Jakarta Sans",
    note: "Modern and narrow.",
  },
  {
    value: "instrument",
    label: "Instrument Sans",
    note: "Warm and easy to read.",
  },
  {
    value: "bricolage",
    label: "Bricolage Grotesque",
    note: "Has a personality of its own.",
  },
  {
    value: "plex",
    label: "IBM Plex Sans",
    note: "The clearest numbers.",
  },
  {
    value: "jetbrains",
    label: "JetBrains Mono",
    note: "Typewriter style: lines run wide.",
  },
  {
    value: "system",
    label: "System (Segoe UI)",
    note: "The font Windows uses.",
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
  const root = document.documentElement;

  root.dataset.font = FONT_OPTIONS.some((option) => option.value === font)
    ? font
    : "googlesans";
}

export function applyWeight(weight: UiWeight): void {
  document.documentElement.dataset.weight = weight;
}
