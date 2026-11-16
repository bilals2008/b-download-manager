import assert from "node:assert/strict";
import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Empty what a build leaves behind.
 *
 * `target/` is the Rust compile cache and grows to tens of gigabytes on this
 * workspace; `dist/` is the web build. Neither is a source, so both go, and
 * the report says what it cost. Nothing else is touched: a clean that guesses
 * is how a repository loses something the reader wanted.
 */
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const TARGETS = ["target", "dist"];

// Only ever clean a workspace this script recognises as one.
assert.ok(existsSync(join(root, "Cargo.toml")), "no Cargo.toml above scripts/");
assert.ok(existsSync(join(root, "package.json")), "no package.json above scripts/");

function sizeOf(path) {
  let total = 0;
  const stack = [path];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const next = join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(next);
      } else if (entry.isFile()) {
        // Symlinks are skipped rather than followed, so a linked folder
        // elsewhere on the disk is measured as the link it is.
        total += statSync(next).size;
      }
    }
  }
  return total;
}

const gb = (bytes) => (bytes / 1024 ** 3).toFixed(2);

let freed = 0;
for (const name of TARGETS) {
  const path = join(root, name);
  if (!existsSync(path)) {
    console.log(`${name}: not there`);
    continue;
  }
  const bytes = sizeOf(path);
  rmSync(path, { recursive: true, force: true });
  assert.ok(!existsSync(path), `${name} is still there`);
  freed += bytes;
  console.log(`${name}: ${gb(bytes)} GB removed`);
}
console.log(`freed ${gb(freed)} GB`);
