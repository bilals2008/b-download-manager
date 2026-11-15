// Language checker for the i18n catalogues.
//
// Two rules the build cannot be allowed to forget:
//   1. every key used in the code exists in the English catalogue;
//   2. every catalogue has the same keys, so a missing word never blanks a
//      window.
// Run with `node scripts/check-i18n.mjs`.

import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const SRC = join(root, "src");

// The strings file is TypeScript; the check reads it with a light hand
// rather than a parser: the catalogue is written as one object literal per
// language with quoted keys, which is stable enough to read this way.
const catalogueSource = readFileSync(join(SRC, "i18n", "catalogue.ts"), "utf8");

function keysOf(literalName) {
  const start = catalogueSource.search(
    new RegExp(`const ${literalName}[^{]*=\\s*\\{`),
  );
  if (start === -1) {
    return new Map();
  }
  const end = catalogueSource.indexOf("\n};", start);
  const body = catalogueSource.slice(start, end);
  const keys = new Map();
  for (const match of body.matchAll(/"([^"]+)":/g)) {
    keys.set(match[1], true);
  }
  return keys;
}

let checks = 0;

function check(condition, label) {
  assert.ok(condition, label);
  checks += 1;
}

const en = keysOf("en");
check(en.size >= 30, "the English catalogue is big enough to be the contract");

for (const name of ["fr", "de", "ar"]) {
  const catalogue = keysOf(name);
  for (const key of en.keys()) {
    check(catalogue.has(key), `${name} is missing the key ${key}`);
  }
  for (const key of catalogue.keys()) {
    check(en.has(key), `${name} has the key ${key} without an English one`);
  }
}

// Every string key used in the code exists in the catalogue. A key is
// anything quoted that looks like a dotted key, in a t() call or a
// catalogue lookup — never a file name or a CSS class.
const used = new Map();
function walk(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      walk(path);
      continue;
    }
    if (!/\.(ts|tsx)$/.test(entry) || entry.endsWith(".test.ts") || entry === "catalogue.ts") {
      continue;
    }
    const source = readFileSync(path, "utf8");
    for (const match of source.matchAll(/\bt\(\s*["']([a-z][a-z0-9-]*(?:\.[a-z0-9-]+)+)["']\s*\)/g)) {
      used.set(match[1], relative(SRC, path));
    }
    for (const match of source.matchAll(/\btranslate\([^,]+,\s*["']([a-z][a-z0-9-]*(?:\.[a-z0-9-]+)+)["']\s*\)/g)) {
      used.set(match[1], relative(SRC, path));
    }
  }
}
walk(SRC);

for (const [key, file] of used) {
  check(en.has(key), `the code uses the key ${key} (${file}), and the catalogue does not have it`);
}

console.log(`check-i18n: ${checks} checks passed, ${used.size} keys used in the code.`);
