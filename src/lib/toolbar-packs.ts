/**
 * Toolbar packs: the appearance of the button bar, and the icons a pack may
 * bring with it.
 *
 * A pack is a JSON file the reader installs from the View menu. The backend
 * checks its shape before it is stored; this module checks it again for the
 * window, turns its raw SVG into image sources and refuses anything that could
 * reach outside the drawing — a pack is content from elsewhere, so it is
 * treated as such.
 */

import {
  DEFAULT_TOOLBAR_ID,
  DEFAULT_TOOLBAR_LAYOUT,
  isToolbarCommand,
  isToolbarPackId,
  normalizeToolbarLayout,
  type ToolbarCommand,
  type ToolbarToken,
} from "./toolbars.ts";
import { TOOLBAR_ICONS_3D } from "./toolbar-art-3d.ts";
import { TOOLBAR_ICONS_CLASSIC } from "./toolbar-art-classic.ts";
import {
  TOOLBAR_ICONS_BLUEPRINT,
  TOOLBAR_ICONS_ISOMETRIC,
} from "./toolbar-art-isometric.ts";

export interface ToolbarPackStyle {
  /** The bar's surface. Unset follows the theme's toolbar token. */
  background?: string;
  /** Label and icon colour. */
  text?: string;
  /** The hover glow and the label of a glowing pack. */
  accent?: string;
  hover?: string;
  active?: string;
  /** The hairline that stands between groups of buttons. */
  line?: string;
  /** Light the icons and labels from behind, with `accent` as the light. */
  glow?: boolean;
}

export interface ToolbarPack {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  style: ToolbarPackStyle;
  /** Ready-to-use image sources, by the command they draw. */
  icons: Partial<Record<ToolbarCommand, string>>;
  /** The pack's suggested layout. Null keeps the application's own. */
  buttons: ToolbarToken[] | null;
  /** Bundled with the application, so it cannot be removed. */
  builtin: boolean;
}

/** The shape the backend sends; every field is checked before it is used. */
export interface RawToolbarPack {
  id: string;
  name: string;
  version?: string | null;
  author?: string | null;
  description?: string | null;
  style?: Partial<Record<keyof ToolbarPackStyle, string | boolean | null>> | null;
  icons?: Record<string, string> | null;
  buttons?: string[] | null;
}

/** The longest icon a pack may carry, before it is drawn. */
const MAX_ICON_BYTES = 16 * 1024;

/** Icons kept from one pack; the backend refuses more as well. */
const MAX_ICONS = 32;

const BLOCKED_ELEMENTS = new Set([
  "script",
  "style",
  "foreignobject",
  "iframe",
  "object",
  "embed",
  "image",
  "audio",
  "video",
  "canvas",
  "link",
  "meta",
]);

/** A colour the CSS engine can be handed as a bare value, nothing else. */
export function isHexColor(value: unknown): value is string {
  return (
    typeof value === "string" &&
    /^#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(value.trim())
  );
}

function clipText(value: unknown, max: number): string {
  if (typeof value !== "string") {
    return "";
  }
  return value
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "")
    .trim()
    .slice(0, max);
}

function normalizeStyle(
  raw: RawToolbarPack["style"],
): ToolbarPackStyle {
  if (!raw || typeof raw !== "object") {
    return {};
  }
  const style: ToolbarPackStyle = {};
  for (const key of ["background", "text", "accent", "hover", "active", "line"] as const) {
    const value = raw[key];
    if (isHexColor(value)) {
      style[key] = value.trim();
    }
  }
  if (typeof raw.glow === "boolean") {
    style.glow = raw.glow;
  }
  return style;
}

function normalizeIcons(
  raw: RawToolbarPack["icons"],
): Partial<Record<ToolbarCommand, string>> {
  const icons: Partial<Record<ToolbarCommand, string>> = {};
  if (!raw || typeof raw !== "object") {
    return icons;
  }
  let count = 0;
  for (const [command, markup] of Object.entries(raw)) {
    if (count >= MAX_ICONS) {
      break;
    }
    if (!isToolbarCommand(command) || typeof markup !== "string") {
      continue;
    }
    const source = sanitizeToolbarSvg(markup);
    if (source) {
      icons[command] = source;
      count += 1;
    }
  }
  return icons;
}

/** The buttons a pack suggests, or null when it leaves the bar alone. */
export function normalizePackButtons(value: unknown): ToolbarToken[] | null {
  const layout = normalizeToolbarLayout(value, []);
  return layout.length > 0 ? layout : null;
}

/**
 * Check one pack from the backend. Null means the file cannot be used; the
 * caller reports the failure instead of showing a half-drawn bar.
 */
export function normalizeToolbarPack(
  raw: RawToolbarPack,
  builtin = false,
): ToolbarPack | null {
  if (!isToolbarPackId(raw?.id)) {
    return null;
  }
  const name = clipText(raw.name, 48);
  if (!name) {
    return null;
  }
  return {
    id: raw.id,
    name,
    version: clipText(raw.version, 24),
    author: clipText(raw.author, 64),
    description: clipText(raw.description, 240),
    style: normalizeStyle(raw.style),
    icons: normalizeIcons(raw.icons),
    buttons: normalizePackButtons(raw.buttons),
    builtin,
  };
}

/* ------------------------------------------------------------- built-ins */

/**
 * The bars that ship with the application. Default keeps the button drawings
 * the shell was built with; 3D Style and Classic are original vector sets in
 * the classic desktop idiom. No artwork from another product is bundled.
 */
const DEFAULT_PACK: ToolbarPack = Object.freeze({
  id: DEFAULT_TOOLBAR_ID,
  name: "Default",
  version: "",
  author: "",
  description: "The button bar that ships with B Download Manager.",
  style: Object.freeze({}),
  icons: Object.freeze({}),
  buttons: null,
  builtin: true,
});

const THREE_D_PACK: ToolbarPack = Object.freeze({
  id: "3d",
  name: "3D Style",
  version: "1.0.0",
  author: "B Download Manager",
  description:
    "Glossy folder, globe, gear and clock buttons with bevels and highlights.",
  style: Object.freeze({}),
  icons: Object.freeze(TOOLBAR_ICONS_3D),
  buttons: null,
  builtin: true,
});

const CLASSIC_PACK: ToolbarPack = Object.freeze({
  id: "classic",
  name: "Classic",
  version: "1.0.0",
  author: "B Download Manager",
  description: "Flat, outlined folder and globe buttons in the older desktop style.",
  style: Object.freeze({}),
  icons: Object.freeze(TOOLBAR_ICONS_CLASSIC),
  buttons: null,
  builtin: true,
});

const ISOMETRIC_PACK: ToolbarPack = Object.freeze({
  id: "isometric",
  name: "Isometric",
  version: "1.0.0",
  author: "B Download Manager",
  description:
    "Chunky isometric slabs with thick ink outlines, painted in riso inks.",
  style: Object.freeze({}),
  icons: Object.freeze(TOOLBAR_ICONS_ISOMETRIC),
  buttons: null,
  builtin: true,
});

const BLUEPRINT_PACK: ToolbarPack = Object.freeze({
  id: "blueprint",
  name: "Blueprint",
  version: "1.0.0",
  author: "B Download Manager",
  description:
    "The same isometric slabs in one blue hue at six lightnesses, like a stamped blueprint.",
  style: Object.freeze({}),
  icons: Object.freeze(TOOLBAR_ICONS_BLUEPRINT),
  buttons: null,
  builtin: true,
});

export const BUILTIN_TOOLBAR_PACKS: readonly ToolbarPack[] = Object.freeze([
  DEFAULT_PACK,
  THREE_D_PACK,
  CLASSIC_PACK,
  ISOMETRIC_PACK,
  BLUEPRINT_PACK,
]);

const BUILTIN_IDS = new Set(BUILTIN_TOOLBAR_PACKS.map((pack) => pack.id));

/** Whether an id belongs to a bar that ships with the application. */
export function isBuiltinToolbarId(id: string): boolean {
  return BUILTIN_IDS.has(id);
}

/** The pack a fresh profile uses, and the one every failure falls back to. */
export const DEFAULT_TOOLBAR_PACK = DEFAULT_PACK;

export function builtinToolbarPack(id: string): ToolbarPack | undefined {
  return BUILTIN_TOOLBAR_PACKS.find((pack) => pack.id === id);
}

/** The layout a pack starts with, used by Customize > Reset. */
export function defaultToolbarLayoutFor(pack: ToolbarPack): ToolbarToken[] {
  return normalizeToolbarLayout(pack.buttons, DEFAULT_TOOLBAR_LAYOUT);
}

/** The custom properties a pack paints with; unset ones fall back in CSS. */
export function toolbarPackStyleVars(
  pack: ToolbarPack,
): Record<`--toolbar-pack-${string}`, string> {
  const vars: Record<string, string> = {};
  const style = pack.style;
  if (style.background) vars["--toolbar-pack-bg"] = style.background;
  if (style.text) vars["--toolbar-pack-text"] = style.text;
  if (style.accent) vars["--toolbar-pack-accent"] = style.accent;
  if (style.hover) vars["--toolbar-pack-hover"] = style.hover;
  if (style.active) vars["--toolbar-pack-active"] = style.active;
  if (style.line) vars["--toolbar-pack-line"] = style.line;
  return vars;
}

/* ------------------------------------------------------------------ icons */

/**
 * Turn the SVG markup of a pack into a data source for an `<img>`.
 *
 * Only a plain drawing survives: scripts, foreign content and external
 * references are removed, and a root without a usable view box is refused so
 * the icon cannot collapse to nothing. The result is drawn by the image
 * pipeline, never inserted as markup, and the window's CSP agrees with that.
 */
export function sanitizeToolbarSvg(markup: string): string | null {
  if (typeof markup !== "string" || markup.length === 0 || markup.length > MAX_ICON_BYTES) {
    return null;
  }
  const text = markup.trim();
  if (!text || !/<svg[\s>]/i.test(text)) {
    return null;
  }
  if (typeof DOMParser === "undefined" || typeof XMLSerializer === "undefined") {
    return null;
  }

  const parsed = new DOMParser().parseFromString(text, "image/svg+xml");
  if (parsed.getElementsByTagName("parsererror").length > 0) {
    return null;
  }
  const root = parsed.documentElement;
  if (!root || root.localName.toLowerCase() !== "svg") {
    return null;
  }

  scrubElement(root);

  if (!root.getAttribute("viewBox")) {
    const width = Number.parseFloat(root.getAttribute("width") ?? "");
    const height = Number.parseFloat(root.getAttribute("height") ?? "");
    if (!(width > 0) || !(height > 0)) {
      return null;
    }
    root.setAttribute("viewBox", `0 0 ${width} ${height}`);
  }
  root.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  root.removeAttribute("width");
  root.removeAttribute("height");

  const cleaned = new XMLSerializer().serializeToString(root);
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(cleaned)}`;
}

function scrubElement(element: Element): void {
  for (const child of Array.from(element.children)) {
    if (BLOCKED_ELEMENTS.has(child.localName.toLowerCase())) {
      child.remove();
      continue;
    }
    scrubElement(child);
  }

  for (const attribute of Array.from(element.attributes)) {
    const name = attribute.name.toLowerCase();
    const local = attribute.localName.toLowerCase();
    const value = attribute.value.trim();
    const lowered = value.toLowerCase();

    const dangerous =
      /^on[a-z]+$/.test(name) ||
      local === "src" ||
      (local === "href" && !value.startsWith("#")) ||
      lowered.includes("javascript:") ||
      // A paint server or filter is fine when it stays inside this drawing.
      /url\s*\(\s*['"]?(?!#)/.test(lowered) ||
      (local === "style" &&
        /expression\s*\(|@import|javascript:/.test(lowered));

    if (dangerous) {
      element.removeAttributeNode(attribute);
    }
  }
}
