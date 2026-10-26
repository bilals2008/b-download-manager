// Self-check for download categories: what a file type detects, which rule
// wins when two claim it, what a category and its folder may be called, and
// what survives a restart. Node strips the types, so this runs against the
// same helpers the pane, the dialogs and the store import. Run it with
// `npm run check:categories`.

import assert from "node:assert/strict";

import {
  BUILTIN_CATEGORIES,
  DEFAULT_CATEGORIES,
  MAX_CATEGORY_EXTENSIONS,
  MAX_CATEGORY_NAME,
  OTHER_CATEGORY,
  categoryDraftProblem,
  categoryFolder,
  categoryFolderProblem,
  categoryName,
  categoryNameProblem,
  cleanCategoryExtension,
  detectCategory,
  invalidCategoryExtensions,
  knownCategory,
  loadCategories,
  normalizeCategories,
  parseCategoryExtensions,
  saveCategories,
} from "../src/lib/categories.ts";
import {
  categoryPaint,
  defaultCategoryIcon,
  normalizeCategoryIcon,
} from "../src/lib/category-icons.ts";
import { matchesStatusFilter } from "../src/lib/downloads.ts";

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

const custom = (name, extensions, folder = "") => ({
  name,
  extensions,
  folder,
  builtin: false,
});

// The built-in rules, by extension, ignoring case and the dot the user may
// type. An unknown type is Other, never an error.
is(detectCategory("film.MP4"), "Video", "the extension is matched without regard to case");
is(detectCategory("film.mp4"), "Video", "a plain video");
is(detectCategory("song.mp3"), "Music", "a plain song");
is(detectCategory("paper.pdf"), "Documents", "a plain document");
is(detectCategory("setup.exe"), "Programs", "a plain program");
is(detectCategory("archive.7z"), "Compressed", "a plain archive");
is(detectCategory("notes.unknownext"), OTHER_CATEGORY, "an unknown type falls back to Other");
is(detectCategory("README"), OTHER_CATEGORY, "a name without a type is Other");
is(detectCategory("trailing."), OTHER_CATEGORY, "an empty extension is Other");
is(detectCategory(""), OTHER_CATEGORY, "an empty name is Other");
is(detectCategory("film.mp4", []), "Video", "no rules at all means the built-ins");

// A user rule wins over a built-in one, so a category the user made can claim
// a type the built-ins already know.
const claimed = [...DEFAULT_CATEGORIES, custom("Courses", ["mp4", "mkv"])];
is(detectCategory("lecture.mp4", claimed), "Courses", "a custom rule wins over Video");
is(detectCategory("song.mp3", claimed), "Music", "the rest of the built-ins still answer");
is(detectCategory("lecture.mp4", [custom("Courses", ["mp4"])]), "Courses", "a lone custom rule");
is(
  detectCategory("x.zip", [custom("Empty", []), ...DEFAULT_CATEGORIES]),
  "Compressed",
  "a custom rule with no types does not take the fallback",
);

// Two user rules for one type cannot both be saved; the first one wins, which
// is what the editor refuses to create in the first place.
const twins = [
  ...DEFAULT_CATEGORIES,
  custom("First", ["xyz"]),
  custom("Second", ["xyz"]),
];
is(detectCategory("a.xyz", twins), "First", "the first custom rule owns the type");

// Extensions are cleaned once, in one place.
is(cleanCategoryExtension(".MP4"), "mp4", "a dot is dropped");
is(cleanCategoryExtension("*.mp4"), "mp4", "a star and a dot are dropped");
is(cleanCategoryExtension("  MP4  "), "mp4", "spaces are trimmed");
is(cleanCategoryExtension("not a type"), "", "a space inside is not a type");
is(cleanCategoryExtension(""), "", "an empty token");
is(cleanCategoryExtension("x".repeat(20)), "", "a token that is too long");
is(
  parseCategoryExtensions("pdf, .DOCX; zip\nmp4"),
  ["pdf", "docx", "zip", "mp4"],
  "a typed list keeps its order and loses duplicates",
);
is(parseCategoryExtensions("pdf, PDF, .pdf"), ["pdf"], "one type is listed once");
is(invalidCategoryExtensions("pdf, ???, docx"), ["???"], "a token that is not a type");
is(invalidCategoryExtensions("pdf, docx"), [], "a list of types has no leftovers");

// A name is a plain word the tree can show and the index can hold.
is(categoryNameProblem("Courses", DEFAULT_CATEGORIES), null, "a new name");
is(categoryNameProblem("   ", DEFAULT_CATEGORIES), "Enter a name for the category.", "an empty name");
is(
  categoryNameProblem("x".repeat(MAX_CATEGORY_NAME + 1), DEFAULT_CATEGORIES),
  `A category name can be at most ${MAX_CATEGORY_NAME} characters.`,
  "a name that is too long",
);
is(
  categoryNameProblem("a:b", DEFAULT_CATEGORIES),
  'A category name cannot contain < > : " / \\ | ? *.',
  "characters the index and the tree cannot show",
);
is(categoryNameProblem("all", DEFAULT_CATEGORIES), "That name belongs to a download view.", "a view name");
is(categoryNameProblem("VIDEO", DEFAULT_CATEGORIES), "A category with that name already exists.", "a built-in name");
is(
  categoryNameProblem("video", DEFAULT_CATEGORIES),
  "A category with that name already exists.",
  "a name that only differs in case",
);
is(
  categoryNameProblem("Video", DEFAULT_CATEGORIES, "Video"),
  null,
  "a category keeps its own name",
);
is(categoryNameProblem("Minhas Aulas", DEFAULT_CATEGORIES), null, "a name with a space");

// A folder is absolute, or empty to follow the default. It does not have to
// exist: the engine creates it with the first file.
is(categoryFolderProblem(""), null, "no folder follows the default");
is(categoryFolderProblem("C:\\Downloads\\Video"), null, "a drive path");
is(categoryFolderProblem("D:/Downloads/Video"), null, "a drive path with forward slashes");
is(categoryFolderProblem("\\\\server\\share\\files"), null, "a network share");
is(categoryFolderProblem("/home/user/files"), null, "a unix path");
is(
  categoryFolderProblem("Downloads\\Video"),
  "Use a full folder path, starting with a drive, a network share or /.",
  "a relative folder would depend on where the app started",
);
is(
  categoryFolderProblem("C:\\a?b"),
  'A folder cannot contain < > " | ? *.',
  "characters a path cannot hold",
);

// A built-in keeps its name and its types. A custom category may claim a
// built-in type on purpose, but not one another custom category owns.
is(
  categoryDraftProblem({ name: "Video", extensions: [], folder: "" }, DEFAULT_CATEGORIES, "Video"),
  null,
  "a built-in folder may change",
);
is(
  categoryDraftProblem(
    { name: "Video", extensions: ["mp4"], folder: "" },
    DEFAULT_CATEGORIES,
    "Video",
  ),
  null,
  "a built-in keeps its own types when they are sent back",
);
is(
  categoryDraftProblem(
    { name: "Films", extensions: ["mp4"], folder: "" },
    DEFAULT_CATEGORIES,
    "Video",
  ),
  "A built-in category cannot be renamed.",
  "a built-in cannot be renamed",
);
is(
  categoryDraftProblem(
    { name: "Courses", extensions: ["mp4"], folder: "" },
    DEFAULT_CATEGORIES,
  ),
  null,
  "a custom rule may claim a built-in type",
);
const withCourses = [...DEFAULT_CATEGORIES, custom("Courses", ["mp4"])];
is(
  categoryDraftProblem({ name: "Talks", extensions: ["mp4"], folder: "" }, withCourses),
  "The .mp4 file type already belongs to Courses.",
  "two custom rules cannot own one type",
);
is(
  categoryDraftProblem(
    { name: "Courses", extensions: ["mp4", "webm"], folder: "" },
    withCourses,
    "Courses",
  ),
  null,
  "a category may keep the type it already owns",
);
is(
  categoryDraftProblem(
    { name: "Courses", extensions: ["mp4"], folder: "relative\\folder" },
    withCourses,
    "Courses",
  ),
  "Use a full folder path, starting with a drive, a network share or /.",
  "a draft folder is checked once",
);

// A stored preference file is brought back to something the tree can trust:
// the built-ins always exist, and nothing ambiguous survives.
const defaults = DEFAULT_CATEGORIES.map((category) => category.name);
is(
  normalizeCategories(undefined).map((category) => category.name),
  defaults,
  "no saved file is the built-in list",
);
is(
  normalizeCategories("not a list").map((category) => category.name),
  defaults,
  "a broken file is the built-in list",
);
const restored = normalizeCategories([
  { name: "Video", extensions: ["mp4"], folder: "  C:\\Downloads\\Video  " },
  { name: "Courses", extensions: [".MP4", "MP4", "webm"], folder: "" },
  { name: "Courses", extensions: ["avi"] },
  { name: "all", extensions: [] },
  { name: "   ", extensions: [] },
  { name: "Second", extensions: ["mp4"], folder: "relative" },
]);
is(
  restored.map((category) => category.name),
  ["Video", "Courses", "Second", OTHER_CATEGORY],
  "unknown, duplicate and reserved names are dropped",
);
is(
  normalizeCategories([]).map((category) => category.name),
  [OTHER_CATEGORY],
  "a built-in the user removed stays removed",
);
is(
  normalizeCategories([{ name: "Courses", extensions: ["xyz"] }]).map(
    (category) => category.name,
  ),
  ["Courses", OTHER_CATEGORY],
  "Other is always restored as the fallback",
);
is(
  categoryFolder(restored, "Video"),
  "C:\\Downloads\\Video",
  "a built-in folder is trimmed and kept",
);
is(
  categoryFolder(restored, "Courses"),
  "",
  "a relative folder is dropped, not saved",
);
is(
  detectCategory("lecture.mp4", restored),
  "Courses",
  "the first custom rule keeps a type two of them claimed",
);
is(
  detectCategory("song.mp3", restored),
  OTHER_CATEGORY,
  "a removed built-in no longer answers for its types",
);
is(
  restored.find((category) => category.name === "Video").extensions,
  DEFAULT_CATEGORIES.find((category) => category.name === "Video").extensions,
  "a saved file cannot shrink a built-in's own types",
);
is(
  restored.every((category) => BUILTIN_CATEGORIES.includes(category.name) === category.builtin),
  true,
  "only the built-ins are marked built in",
);

// An icon is two ids the app knows. A saved one comes back as it was; a bad
// one falls back to the drawing the category has always had.
is(
  restored.find((category) => category.name === "Video").icon,
  { glyph: "video", palette: "red" },
  "a built-in keeps its icon",
);
const painted = normalizeCategories([
  { name: "Courses", extensions: ["xyz"], icon: { glyph: "star", palette: "green" } },
  { name: "Second", extensions: [], icon: { glyph: "nope", palette: "#ff0000" } },
  { name: "Video", icon: { glyph: "cloud", palette: "blue" } },
]);
is(
  painted.find((category) => category.name === "Courses").icon,
  { glyph: "star", palette: "green" },
  "a saved icon comes back",
);
is(
  painted.find((category) => category.name === "Second").icon,
  defaultCategoryIcon("Second"),
  "an unknown icon falls back to the name's default",
);
is(
  painted.find((category) => category.name === "Video").icon,
  { glyph: "cloud", palette: "blue" },
  "a built-in icon can be changed",
);

// A colour of the user's own is a plain hex, and anything else is dropped so a
// saved preference cannot paint the icon with something CSS would not accept.
is(
  normalizeCategoryIcon(
    { glyph: "star", palette: "green", color: "#FF8800" },
    defaultCategoryIcon("Tone"),
  ).color,
  "#ff8800",
  "a custom colour is kept, lowercased",
);
is(
  normalizeCategoryIcon(
    { glyph: "star", palette: "green", color: "url(x)" },
    defaultCategoryIcon("Tone"),
  ),
  { glyph: "star", palette: "green" },
  "a colour that is not a hex is dropped",
);
is(
  normalizeCategories([
    {
      name: "Tone",
      extensions: [],
      icon: { glyph: "star", palette: "green", color: "#123456" },
    },
  ]).find((category) => category.name === "Tone").icon,
  { glyph: "star", palette: "green", color: "#123456" },
  "a custom colour survives a save",
);

// The glyph is white on a palette plate and on a dark custom colour, and turns
// dark when the user picks a pale one, so it never disappears.
is(
  categoryPaint({ glyph: "star", palette: "green" }).ink,
  "#fff",
  "a palette plate keeps the white glyph",
);
is(
  categoryPaint({ glyph: "star", palette: "green", color: "#f5e7a1" }).ink,
  "#1b1b1b",
  "a pale custom plate darkens the glyph",
);
is(
  categoryPaint({ glyph: "star", palette: "green", color: "#123456" }).ink,
  "#fff",
  "a dark custom plate keeps the white glyph",
);
is(
  normalizeCategories([
    { name: "Courses", extensions: Array.from({ length: MAX_CATEGORY_EXTENSIONS + 10 }, (_, at) => `t${at}`) },
  ]).find((category) => category.name === "Courses").extensions.length,
  MAX_CATEGORY_EXTENSIONS,
  "a saved list is cut to the limit",
);

// A name stored in the index is only used while it is still configured.
is(knownCategory("Video", restored), "Video", "a configured name");
is(knownCategory("Gone", restored), null, "a removed category is no category");
is(knownCategory(null, restored), null, "no name");
is(knownCategory("", restored), null, "an empty name");
is(categoryName("Video"), "Video", "an older index keeps its built-in name");
is(categoryName("Courses"), null, "an older index has no custom name");
is(categoryFolder(restored, "Nope"), "", "a missing category has no folder");

// The definitions and the folders survive a restart, and a broken file does
// not stop the app.
const storage = new Map();
globalThis.window = {
  localStorage: {
    getItem: (key) => storage.get(key) ?? null,
    setItem: (key, value) => storage.set(key, String(value)),
  },
};
is(
  loadCategories().map((category) => category.name),
  defaults,
  "nothing saved yet is the built-in list",
);
saveCategories([
  ...DEFAULT_CATEGORIES,
  custom("Courses", ["mp4"], "C:\\Downloads\\Courses"),
]);
const reloaded = loadCategories();
is(reloaded.length, DEFAULT_CATEGORIES.length + 1, "a custom category comes back");
is(categoryFolder(reloaded, "Courses"), "C:\\Downloads\\Courses", "its folder comes back");
is(detectCategory("lecture.mp4", reloaded), "Courses", "its rule still detects");
storage.set("idm-clone.categories.v1", "{ this is not json");
is(
  loadCategories().map((category) => category.name),
  defaults,
  "a corrupted file is the built-in list",
);
storage.set("idm-clone.categories.v1", JSON.stringify({ version: 1 }));
is(
  loadCategories().map((category) => category.name),
  defaults,
  "a file without categories is the built-in list",
);

// The status filter narrows the rows the category filter already chose.
const row = { status: "paused" };
is(matchesStatusFilter(row, "all"), true, "all statuses match");
is(matchesStatusFilter(row, "paused"), true, "the row's own status");
is(matchesStatusFilter(row, "completed"), false, "another status");
is(matchesStatusFilter({ status: "completed" }, "completed"), true, "a finished row");

console.log(`Download categories: ${checks} checks pass`);
