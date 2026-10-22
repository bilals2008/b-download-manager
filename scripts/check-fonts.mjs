// Font gate. The appearance page offers a fixed list of faces, and every one
// of them has to reach three places or the picker silently does nothing:
// a `--app-font-ui` stack in src/index.css, the files those stacks name, and a
// `data-font` value the settings module accepts. Run it with
// `npm run check:fonts`.

import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

const read = (path) =>
  readFileSync(fileURLToPath(new URL(`../${path}`, import.meta.url)), "utf8");

const css = read("src/index.css");
const fonts = read("src/lib/fonts.ts");
const settings = read("src/lib/settings.ts");
const failures = [];

// 1. Every file a face is loaded from is in the repository.
const files = [...css.matchAll(/url\("\.\/(assets\/fonts\/[^"]+)"\)/g)].map(
  (match) => match[1],
);
if (files.length === 0) {
  failures.push("no @font-face files found in src/index.css");
}
for (const file of files) {
  if (!existsSync(fileURLToPath(new URL(`../src/${file}`, import.meta.url)))) {
    failures.push(`missing font file: src/${file}`);
  }
}

// 2. The base block names a face and a weight, so the window has both before
//    any choice.
if (!/:root\s*\{[^}]*--app-font-ui:/.test(css)) {
  failures.push(":root has no --app-font-ui stack");
}
if (!/:root\s*\{[^}]*--app-font-weight:/.test(css)) {
  failures.push(":root has no --app-font-weight step");
}

// 3. Every option the picker lists has a stack, and the settings module keeps
//    it after a reload. Inter is the base stack, so it needs no attribute rule.
const options = [
  ...fonts.slice(fonts.indexOf("FONT_OPTIONS"), fonts.indexOf("WEIGHT_OPTIONS"))
    .matchAll(/value:\s*"([a-z]+)"/g),
].map((match) => match[1]);
if (options.length === 0) {
  failures.push("no options parsed out of src/lib/fonts.ts");
}
for (const option of options) {
  if (!settings.includes(`"${option}"`)) {
    failures.push(`src/lib/settings.ts does not accept the face "${option}"`);
  }
  if (option !== "inter" && !css.includes(`:root[data-font="${option}"]`)) {
    failures.push(`no :root[data-font="${option}"] stack in src/index.css`);
  }
  if (option === "inter" && !/:root\s*\{[^}]*--app-font-ui:/.test(css)) {
    failures.push(`no base --app-font-ui stack for "inter"`);
  }
}

// 4. The weights. Regular is the base step, so it needs no attribute rule.
const weights = [
  ...fonts
    .slice(fonts.indexOf("WEIGHT_OPTIONS"), fonts.indexOf("SIZE_OPTIONS"))
    .matchAll(/value:\s*"([a-z]+)"/g),
].map((match) => match[1]);
if (weights.length === 0) {
  failures.push("no weight options parsed out of src/lib/fonts.ts");
}
for (const weight of weights) {
  if (!settings.includes(`"${weight}"`)) {
    failures.push(`src/lib/settings.ts does not accept the weight "${weight}"`);
  }
  if (weight !== "regular" && !css.includes(`:root[data-weight="${weight}"]`)) {
    failures.push(`no :root[data-weight="${weight}"] scale in src/index.css`);
  }
}

// 5. The sizes, the same way. Default is the base step.
const sizes = [
  ...fonts
    .slice(fonts.indexOf("SIZE_OPTIONS"))
    .matchAll(/value:\s*"([a-z]+)"/g),
].map((match) => match[1]);
if (sizes.length === 0) {
  failures.push("no size options parsed out of src/lib/fonts.ts");
}
for (const size of sizes) {
  if (!settings.includes(`"${size}"`)) {
    failures.push(`src/lib/settings.ts does not accept the size "${size}"`);
  }
  if (size !== "default" && !css.includes(`:root[data-size="${size}"]`)) {
    failures.push(`no :root[data-size="${size}"] scale in src/index.css`);
  }
}

// 6. The face is a reference, not a compiled-in value; a literal font list in
//    the theme block would freeze the picker.
if (!/--font-sans:\s*var\(--app-font-ui\)/.test(css)) {
  failures.push("--font-sans is not a var(--app-font-ui) reference");
}
if (!/--font-weight-medium:\s*var\(--app-font-weight-medium\)/.test(css)) {
  failures.push("--font-weight-medium does not follow the weight scale");
}
if (!/--text-xs:\s*calc\([^)]*var\(--app-text-scale\)/.test(css)) {
  failures.push("--text-xs does not follow the text scale");
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`FAIL     ${failure}`);
  }
  process.exit(1);
}

console.log(
  `fonts    ${options.length} faces, ${files.length} files, ` +
    `${weights.length} weights, ${sizes.length} sizes, all wired`,
);
