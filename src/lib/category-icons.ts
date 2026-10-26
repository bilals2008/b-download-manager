/**
 * The category icon system: a fixed set of drawn glyphs, six theme palettes and
 * an optional colour of the user's own. A category stores ids, never a drawing,
 * so the artwork stays in one place and a saved preference cannot inject SVG.
 *
 * Adding a glyph means one entry here and one drawing in `desktop-icons`.
 * Adding a palette means one entry here; the token triple already exists.
 */

export type CategoryPaletteId =
  | "amber"
  | "red"
  | "purple"
  | "blue"
  | "green"
  | "silver";

export interface CategoryPalette {
  id: CategoryPaletteId;
  label: string;
  bright: string;
  base: string;
  deep: string;
}

export const CATEGORY_PALETTES: Record<CategoryPaletteId, CategoryPalette> = {
  amber: {
    id: "amber",
    label: "Amber",
    bright: "--icon-amber-bright",
    base: "--icon-amber",
    deep: "--icon-amber-deep",
  },
  red: {
    id: "red",
    label: "Red",
    bright: "--icon-red-bright",
    base: "--icon-red",
    deep: "--icon-red-deep",
  },
  purple: {
    id: "purple",
    label: "Purple",
    bright: "--icon-purple-bright",
    base: "--icon-purple",
    deep: "--icon-purple-deep",
  },
  blue: {
    id: "blue",
    label: "Blue",
    bright: "--icon-blue-bright",
    base: "--icon-blue",
    deep: "--icon-blue-deep",
  },
  green: {
    id: "green",
    label: "Green",
    bright: "--icon-green-bright",
    base: "--icon-green",
    deep: "--icon-green-deep",
  },
  silver: {
    id: "silver",
    label: "Silver",
    bright: "--icon-silver-bright",
    base: "--icon-silver",
    deep: "--icon-silver-deep",
  },
};

/** The picker's order: the six palettes the theme already draws. */
export const CATEGORY_PALETTE_IDS: CategoryPaletteId[] = [
  "amber",
  "red",
  "purple",
  "blue",
  "green",
  "silver",
];

export type CategoryGlyphId =
  | "folder"
  | "page"
  | "video"
  | "music"
  | "document"
  | "program"
  | "game"
  | "archive"
  | "image"
  | "code"
  | "book"
  | "cloud"
  | "globe"
  | "star"
  | "box"
  | "ellipsis";

/** The words the picker shows for each drawing. */
export const CATEGORY_GLYPHS: Record<CategoryGlyphId, string> = {
  folder: "Folder",
  page: "Page",
  video: "Video",
  music: "Music",
  document: "Document",
  program: "Program",
  game: "Game",
  archive: "Archive",
  image: "Image",
  code: "Code",
  book: "Book",
  cloud: "Cloud",
  globe: "Globe",
  star: "Star",
  box: "Box",
  ellipsis: "More",
};

/** The picker's order: two rows of eight on a form width. */
export const CATEGORY_GLYPH_IDS: CategoryGlyphId[] = [
  "folder",
  "page",
  "video",
  "music",
  "document",
  "program",
  "game",
  "archive",
  "image",
  "code",
  "book",
  "cloud",
  "globe",
  "star",
  "box",
  "ellipsis",
];

/** What a category remembers: a drawing, a palette and optionally its own colour. */
export interface CategoryIcon {
  glyph: CategoryGlyphId;
  palette: CategoryPaletteId;
  /** `#rrggbb` picked by the user; absent means the palette above paints it. */
  color?: string;
}

/** Where the custom colour picker starts before the user has chosen one. */
export const DEFAULT_CUSTOM_COLOR = "#3f7bd9";

/** The icon each built-in category has always shown. */
export const BUILTIN_ICONS: Record<string, CategoryIcon> = {
  General: { glyph: "page", palette: "amber" },
  Video: { glyph: "video", palette: "red" },
  Music: { glyph: "music", palette: "purple" },
  Documents: { glyph: "document", palette: "blue" },
  Programs: { glyph: "program", palette: "green" },
  Games: { glyph: "game", palette: "silver" },
  Compressed: { glyph: "archive", palette: "amber" },
  Other: { glyph: "ellipsis", palette: "silver" },
};

/** Palettes a category the user made borrows from, in picker order. */
const CUSTOM_PALETTES: CategoryPaletteId[] = [
  "blue",
  "green",
  "purple",
  "amber",
  "red",
  "silver",
];

export function isCategoryGlyph(value: unknown): value is CategoryGlyphId {
  return typeof value === "string" && Object.hasOwn(CATEGORY_GLYPHS, value);
}

export function isCategoryPalette(value: unknown): value is CategoryPaletteId {
  return typeof value === "string" && Object.hasOwn(CATEGORY_PALETTES, value);
}

/** A colour a saved preference may keep: a plain six-digit hex, lowercased. */
export function normalizeHexColor(value: unknown): string | null {
  if (typeof value !== "string" || !/^#[0-9a-f]{6}$/i.test(value.trim())) {
    return null;
  }
  return value.trim().toLowerCase();
}

/**
 * The three stops an icon is painted with, as CSS colour values. A palette is
 * two theme variables; a custom colour gets a lighter and a darker stop mixed
 * from the one the user picked, so the gloss and the hairline still read.
 */
export interface CategoryPaint {
  bright: string;
  base: string;
  deep: string;
  /** The glyph ink: white on a dark plate, near-black on a light custom one. */
  ink: string;
}

/** Dark enough to read on a pale plate the user picked. */
const LIGHT_INK = "#1b1b1b";

export function categoryPaint(icon: CategoryIcon): CategoryPaint {
  if (icon.color) {
    return {
      bright: mixColor(icon.color, 255, 0.22),
      base: icon.color,
      deep: mixColor(icon.color, 0, 0.3),
      ink: isLightColor(icon.color) ? LIGHT_INK : "#fff",
    };
  }
  const palette = CATEGORY_PALETTES[icon.palette];
  return {
    bright: palette.bright,
    base: palette.base,
    deep: palette.deep,
    ink: "#fff",
  };
}

/** Whether a hex reads as a pale colour, so the glyph must turn dark. */
function isLightColor(hex: string): boolean {
  const [red, green, blue] = [1, 3, 5].map(
    (at) => parseInt(hex.slice(at, at + 2), 16) / 255,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue > 0.62;
}

/** One hex channel moved towards `target` by `amount` (0 to 1). */
function mixColor(hex: string, target: number, amount: number): string {
  const channels = [1, 3, 5].map((at) => {
    const value = parseInt(hex.slice(at, at + 2), 16);
    return Math.round(value + (target - value) * amount)
      .toString(16)
      .padStart(2, "0");
  });
  return `#${channels.join("")}`;
}

/**
 * The icon of a category that has none saved: the built-in drawing it has
 * always had, or a folder tinted from its name so two custom categories are
 * still apart before the user picks anything.
 */
export function defaultCategoryIcon(name: string): CategoryIcon {
  if (Object.hasOwn(BUILTIN_ICONS, name)) {
    return BUILTIN_ICONS[name];
  }
  let hash = 0;
  for (const char of name) {
    hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  }
  return {
    glyph: "folder",
    palette: CUSTOM_PALETTES[hash % CUSTOM_PALETTES.length],
  };
}

/** Bring a saved pair back to ids the app knows, or use the fallback. */
export function normalizeCategoryIcon(
  value: unknown,
  fallback: CategoryIcon,
): CategoryIcon {
  if (!value || typeof value !== "object") {
    return fallback;
  }
  const saved = value as Partial<CategoryIcon>;
  const color = normalizeHexColor(saved.color);
  return {
    glyph: isCategoryGlyph(saved.glyph) ? saved.glyph : fallback.glyph,
    palette: isCategoryPalette(saved.palette) ? saved.palette : fallback.palette,
    ...(color ? { color } : {}),
  };
}
