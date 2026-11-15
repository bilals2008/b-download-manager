// Every script parses and every stylesheet has balanced braces.

import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { rootDir } from "./common.mjs";

function everyFile(dir, suffix) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) out.push(...everyFile(full, suffix));
    else if (full.endsWith(suffix)) out.push(full);
  }
  return out;
}

for (const file of everyFile(rootDir, ".js")) {
  // Parsing only: the scripts need a browser, a page or both to run.
  new vm.Script(readFileSync(file, "utf8"), { filename: file });
}

for (const file of everyFile(rootDir, ".css")) {
  const text = readFileSync(file, "utf8");
  const open = (text.match(/{/g) || []).length;
  const close = (text.match(/}/g) || []).length;
  assert.equal(open, close, `${path.relative(rootDir, file)} has balanced braces`);
}