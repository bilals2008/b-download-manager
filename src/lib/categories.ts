/**
 * Download categories: the built-in file types, the rules a user adds, and
 * where a category saves its files.
 *
 * The definitions are a preference, kept beside the settings. The category a
 * download belongs to is not: the engine stores it in the download index, so a
 * rename or a removal changes the index only. It never moves a file on disk.
 */

import {
  defaultCategoryIcon,
  normalizeCategoryIcon,
  type CategoryIcon,
} from "./category-icons.ts";

export const BUILTIN_CATEGORIES = [
  "General",
  "Video",
  "Music",
  "Documents",
  "Programs",
  "Games",
  "Compressed",
  "Other",
] as const;

export type BuiltinCategory = (typeof BUILTIN_CATEGORIES)[number];

/** A category name is a configured name, not only one of the built-ins. */
export type CategoryName = string;

/** The built-in names, in the order the tree shows them. */
export const CATEGORIES = BUILTIN_CATEGORIES;

export const OTHER_CATEGORY: BuiltinCategory = "Other";

export interface CategoryDefinition {
  name: string;
  /** Lower-case extensions without the dot. Empty matches nothing. */
  extensions: string[];
  /** An absolute folder, or empty to use the default save folder. */
  folder: string;
  builtin: boolean;
  /** The drawing and palette the tree shows for it. */
  icon: CategoryIcon;
}

export interface CategoryDraft {
  name: string;
  extensions: string[];
  folder: string;
  icon: CategoryIcon;
}

/** The file types a built-in category owns before any user rule. */
const BUILTIN_EXTENSIONS: Record<BuiltinCategory, readonly string[]> = {
  General: [],
  Video: ["mp4", "mkv", "avi", "mov", "webm", "m4v", "mpg", "mpeg"],
  Music: ["mp3", "flac", "wav", "m4a", "ogg", "aac"],
  Documents: [
    "pdf",
    "doc",
    "docx",
    "txt",
    "rtf",
    "xls",
    "xlsx",
    "ppt",
    "pptx",
    "csv",
    "epub",
  ],
  Programs: [
    "exe",
    "msi",
    "msix",
    "dmg",
    "pkg",
    "apk",
    "deb",
    "rpm",
    "appimage",
  ],
  Games: [],
  Compressed: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
  Other: [],
};

export const DEFAULT_CATEGORIES: CategoryDefinition[] = BUILTIN_CATEGORIES.map(
  (name) => ({
    name,
    extensions: [...BUILTIN_EXTENSIONS[name]],
    folder: "",
    builtin: true,
    icon: defaultCategoryIcon(name),
  }),
);

export const MAX_CATEGORY_NAME = 40;
export const MAX_CATEGORY_EXTENSIONS = 64;

const STORAGE_KEY = "idm-clone.categories.v1";
const STORAGE_VERSION = 1;
const EXTENSION_PATTERN = /^[a-z0-9][a-z0-9+_-]{0,15}$/;
const RESERVED_NAMES = ["all", "unfinished", "finished"];

function cloneDefaults(): CategoryDefinition[] {
  return DEFAULT_CATEGORIES.map((category) => ({
    ...category,
    extensions: [...category.extensions],
  }));
}

/** ".MP4", "*.mp4" and " mp4 " are the same extension. */
export function cleanCategoryExtension(value: string): string {
  const cleaned = value.trim().toLowerCase().replace(/^\*?\./, "");
  return EXTENSION_PATTERN.test(cleaned) ? cleaned : "";
}

/** The extensions a typed list names, in the order they were typed. */
export function parseCategoryExtensions(text: string): string[] {
  const out: string[] = [];
  for (const token of text.split(/[\s,;]+/)) {
    const extension = cleanCategoryExtension(token);
    if (extension && !out.includes(extension)) {
      out.push(extension);
    }
    if (out.length >= MAX_CATEGORY_EXTENSIONS) {
      break;
    }
  }
  return out;
}

/** The tokens of a typed list that are not file types at all. */
export function invalidCategoryExtensions(text: string): string[] {
  return text
    .split(/[\s,;]+/)
    .filter((token) => token.trim() !== "" && !cleanCategoryExtension(token))
    .slice(0, 8);
}

/** The extension of a file name, without the dot. */
function extensionOf(fileName: string): string {
  const dot = fileName.lastIndexOf(".");
  return dot < 0 ? "" : cleanCategoryExtension(fileName.slice(dot + 1));
}

/**
 * The category a file name belongs to.
 *
 * A user rule wins over a built-in rule, so a custom category can claim a type
 * the built-ins already know. Two user rules for the same type cannot both be
 * saved; the first one in the list wins if an old preference file has both.
 * An unknown or missing extension is Other, never an error.
 */
export function detectCategory(
  fileName: string,
  categories: readonly CategoryDefinition[] = DEFAULT_CATEGORIES,
): CategoryName {
  const available = categories.length > 0 ? categories : DEFAULT_CATEGORIES;
  const extension = extensionOf(fileName);
  if (extension) {
    for (const category of available) {
      if (!category.builtin && category.extensions.includes(extension)) {
        return category.name;
      }
    }
    for (const category of available) {
      if (category.extensions.includes(extension)) {
        return category.name;
      }
    }
  }
  return (
    available.find((category) => category.name === OTHER_CATEGORY)?.name ??
    available[available.length - 1]?.name ??
    OTHER_CATEGORY
  );
}

/** Best-effort category for a file name, so the categories pane stays useful. */
export function categoryOfFile(
  fileName: string,
  categories: readonly CategoryDefinition[] = DEFAULT_CATEGORIES,
): CategoryName {
  return detectCategory(fileName, categories);
}

/**
 * The category a stored download keeps, when its name is still configured.
 * A name left behind by a removed category is no category at all.
 */
export function knownCategory(
  value: string | null | undefined,
  categories: readonly CategoryDefinition[],
): CategoryName | null {
  if (!value) {
    return null;
  }
  return categories.some((category) => category.name === value) ? value : null;
}

/** A built-in category when a value from an older index is one. */
export function categoryName(
  value: string | null | undefined,
): BuiltinCategory | null {
  return BUILTIN_CATEGORIES.includes(value as BuiltinCategory)
    ? (value as BuiltinCategory)
    : null;
}

/** The folder a category saves new downloads to. Empty follows the default. */
export function categoryFolder(
  categories: readonly CategoryDefinition[],
  name: string,
): string {
  return (
    categories.find((category) => category.name === name)?.folder.trim() ?? ""
  );
}

/** The icon a category shows, falling back to the one its name has always had. */
export function categoryIcon(
  categories: readonly CategoryDefinition[],
  name: string,
): CategoryIcon {
  return (
    categories.find((category) => category.name === name)?.icon ??
    defaultCategoryIcon(name)
  );
}

/** What is wrong with a category name, or null when it can be used. */
export function categoryNameProblem(
  name: string,
  categories: readonly CategoryDefinition[],
  originalName?: string,
): string | null {
  const trimmed = name.trim();
  if (!trimmed) {
    return "Enter a name for the category.";
  }
  if (trimmed.length > MAX_CATEGORY_NAME) {
    return `A category name can be at most ${MAX_CATEGORY_NAME} characters.`;
  }
  if (/[<>:"/\\|?*\u0000-\u001f]/.test(trimmed)) {
    return 'A category name cannot contain < > : " / \\ | ? *.';
  }
  if (RESERVED_NAMES.includes(trimmed.toLowerCase())) {
    return "That name belongs to a download view.";
  }
  const duplicate = categories.some(
    (category) =>
      category.name !== originalName &&
      category.name.toLowerCase() === trimmed.toLowerCase(),
  );
  return duplicate ? "A category with that name already exists." : null;
}

/**
 * What is wrong with a destination folder, or null when it can be used.
 *
 * The folder does not have to exist: the engine creates it with the first file.
 * A relative path is refused because it would depend on where the app was
 * started, not on what the user chose.
 */
export function categoryFolderProblem(folder: string): string | null {
  const trimmed = folder.trim();
  if (!trimmed) {
    return null;
  }
  if (/[<>"|?*\u0000-\u001f]/.test(trimmed)) {
    return 'A folder cannot contain < > " | ? *.';
  }
  if (!/^(?:[a-zA-Z]:[\\/]|\\\\|\/)/.test(trimmed)) {
    return "Use a full folder path, starting with a drive, a network share or /.";
  }
  return null;
}

/** What is wrong with a category before it is saved, or null. */
export function categoryDraftProblem(
  draft: CategoryDraft,
  categories: readonly CategoryDefinition[],
  originalName?: string,
): string | null {
  const original = originalName
    ? categories.find((category) => category.name === originalName)
    : undefined;
  if (original?.builtin && draft.name.trim() !== original.name) {
    return "A built-in category cannot be renamed.";
  }

  const nameProblem = categoryNameProblem(
    draft.name,
    categories,
    originalName,
  );
  if (nameProblem) {
    return nameProblem;
  }

  const folderProblem = categoryFolderProblem(draft.folder);
  if (folderProblem) {
    return folderProblem;
  }

  // A built-in's types are fixed. A custom category may deliberately claim a
  // built-in type, but not one another custom category already owns.
  if (!original?.builtin) {
    for (const extension of draft.extensions) {
      const owner = categories.find(
        (category) =>
          category.name !== originalName &&
          category.extensions.includes(extension) &&
          !category.builtin,
      );
      if (owner) {
        return `The .${extension} file type already belongs to ${owner.name}.`;
      }
    }
  }
  return null;
}

/**
 * Bring a saved preference file back in a shape the app can trust.
 *
 * The saved file names the categories that exist: a built-in the user removed
 * stays removed instead of returning on the next start. Other is the fallback
 * every unmatched file lands in, so it is always restored. Unknown names,
 * duplicate names, and types another custom category owns are dropped rather
 * than allowed to make the tree ambiguous. A bad file is the defaults, never a
 * failed start.
 */
export function normalizeCategories(value: unknown): CategoryDefinition[] {
  const defaults = cloneDefaults();
  if (!Array.isArray(value)) {
    return defaults;
  }

  const builtinByName = new Map(
    defaults.map((category) => [category.name, category]),
  );
  const out: CategoryDefinition[] = [];
  const seen = new Set<string>();

  /** A folder is kept only when it is absolute; a bad one falls back. */
  const savedFolder = (saved: Partial<CategoryDefinition>) =>
    typeof saved.folder === "string" && !categoryFolderProblem(saved.folder)
      ? saved.folder.trim()
      : "";

  // Built-ins first, in the order they were saved, so the tree keeps its
  // familiar top and a built-in owns its file types.
  for (const entry of value) {
    if (!entry || typeof entry !== "object") {
      continue;
    }
    const saved = entry as Partial<CategoryDefinition>;
    const name = typeof saved.name === "string" ? saved.name.trim() : "";
    const builtin = builtinByName.get(name);
    // `builtin: false` is a user category that took the name of a removed
    // built-in; it is restored in the second pass as the user made it.
    if (!builtin || saved.builtin === false || seen.has(name)) {
      continue;
    }
    builtin.folder = savedFolder(saved);
    builtin.icon = normalizeCategoryIcon(saved.icon, builtin.icon);
    out.push(builtin);
    seen.add(name);
  }

  for (const entry of value) {
    if (!entry || typeof entry !== "object") {
      continue;
    }
    const saved = entry as Partial<CategoryDefinition>;
    const name = typeof saved.name === "string" ? saved.name.trim() : "";
    if (!name || seen.has(name) || categoryNameProblem(name, out) !== null) {
      continue;
    }
    const extensions: string[] = [];
    if (Array.isArray(saved.extensions)) {
      for (const raw of saved.extensions) {
        if (typeof raw !== "string") {
          continue;
        }
        const extension = cleanCategoryExtension(raw);
        if (
          extension &&
          !extensions.includes(extension) &&
          // A custom rule may claim a built-in type on purpose; it may only
          // not claim one another custom category already owns.
          !out.some(
            (category) =>
              !category.builtin && category.extensions.includes(extension),
          )
        ) {
          extensions.push(extension);
        }
        if (extensions.length >= MAX_CATEGORY_EXTENSIONS) {
          break;
        }
      }
    }
    out.push({
      name,
      extensions,
      folder: savedFolder(saved),
      builtin: false,
      icon: normalizeCategoryIcon(
        saved.icon,
        defaultCategoryIcon(name),
      ),
    });
    seen.add(name);
  }

  // Other holds every file no rule claims; without it a download would have
  // nowhere to land.
  if (!out.some((category) => category.name === OTHER_CATEGORY)) {
    out.push(builtinByName.get(OTHER_CATEGORY) as CategoryDefinition);
  }

  return out;
}

/** Read the saved categories, falling back to the built-ins on bad data. */
export function loadCategories(): CategoryDefinition[] {
  try {
    if (typeof window === "undefined") {
      return cloneDefaults();
    }
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return cloneDefaults();
    }
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return cloneDefaults();
    }
    return normalizeCategories(
      (parsed as { categories?: unknown }).categories,
    );
  } catch {
    return cloneDefaults();
  }
}

export function saveCategories(categories: readonly CategoryDefinition[]): void {
  try {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        version: STORAGE_VERSION,
        categories,
      }),
    );
  } catch {
    // Storage can be blocked; the categories still work for this session.
  }
}
