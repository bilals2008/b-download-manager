#!/usr/bin/env node
/*
  The file-size guard.

  A file that grows without a limit becomes the one file nobody dares touch.
  The limit here is 800 lines; `scripts/check-file-size.mjs` fails when a
  source file is over it.

  Files that are being split are listed in the allowlist below with the wave
  that splits them. An entry is removed when the split has landed.

  Run it with `pnpm check:file-size`.
*/

import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The longest a source file may be. */
const LIMIT = 800;

/** Folders that hold no hand-written code. */
const SKIP = new Set([
  "node_modules",
  "target",
  "dist",
  ".git",
  "assets",
  "fonts",
  "locales",
  "icons",
]);

/** The folders that are scanned. */
const ROOTS = ["src", "src-tauri/src", "crates", "scripts"];

/**
 * Files that are over the limit and are being split. Each entry names the
 * wave that splits it; the entry goes when the split has landed.
 */
const ALLOWLIST = new Map([
  ["crates/idm-core/src/downloader.rs", "wave 1"],
  ["crates/idm-core/src/downloader/tests.rs", "wave 1"],
  ["crates/idm-core/src/media/download.rs", "wave 1"],
  ["crates/idm-core/src/media/dash.rs", "wave 1"],
  ["crates/idm-core/src/media/hls.rs", "wave 1"],
  ["crates/idm-core/src/http.rs", "wave 1"],
  ["src-tauri/src/video/mp4.rs", "wave 1"],
  ["src-tauri/src/video/verify.rs", "wave 1"],
  ["src-tauri/src/video/webm.rs", "wave 1"],
  ["src-tauri/src/video/download.rs", "wave 1"],
  ["src-tauri/src/video/yt.rs", "wave 1"],
  ["src-tauri/src/video/capture.rs", "wave 1"],
  ["src-tauri/src/grabber/crawl.rs", "wave 1"],
  ["src-tauri/src/grabber/policy.rs", "wave 1"],
  ["src-tauri/src/actions.rs", "wave 1"],
  ["src-tauri/src/state.rs", "wave 1"],
  ["src-tauri/src/lib.rs", "wave 1"],
  ["src/state/download-store.tsx", "wave 1"],
  ["src/lib/shortcuts.ts", "wave 1"],
  ["src/App.tsx", "wave 1"],
  ["scripts/check-extension.mjs", "wave 1"],
  ["src-tauri/src/bridge/mod.rs", "wave 1"],
]);

const SOURCE_EXTENSIONS = new Set([".rs", ".ts", ".tsx", ".mjs"]);

/** Every source file under the roots. */
function* sources(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP.has(entry.name)) {
        continue;
      }
      yield* sources(join(dir, entry.name));
      continue;
    }
    if (!entry.isFile() || !SOURCE_EXTENSIONS.has(extname(entry.name))) {
      continue;
    }
    yield join(dir, entry.name);
  }
}

const offenders = [];
for (const name of ROOTS) {
  const dir = join(root, name);
  let stat;
  try {
    stat = statSync(dir);
  } catch {
    continue;
  }
  if (!stat.isDirectory()) {
    continue;
  }
  for (const file of sources(dir)) {
    const lines = readFileSync(file, "utf8").split("\n").length;
    if (lines <= LIMIT) {
      continue;
    }
    const path = relative(root, file).replaceAll("\\", "/");
    offenders.push({ path, lines, allowed: ALLOWLIST.has(path) });
  }
}

const hard = offenders.filter((entry) => !entry.allowed);
const soft = offenders.filter((entry) => entry.allowed);

if (hard.length) {
  console.error(`file size: ${hard.length} file(s) over ${LIMIT} lines`);
  for (const entry of hard) {
    console.error(`  ${entry.lines}  ${entry.path}`);
  }
  console.error(
    "\nSplit them before the change: one responsibility per file, and the code moves rather than being rewritten.",
  );
  assert.equal(hard.length, 0, "a source file is over the size limit");
}

if (soft.length) {
  console.log(
    `file size: ${soft.length} file(s) over ${LIMIT} lines, listed for a split`,
  );
  for (const entry of soft) {
    console.log(`  ${entry.lines}  ${entry.path}  (${ALLOWLIST.get(entry.path)})`);
  }
}

console.log(
  soft.length
    ? `file size: no unlisted violations; ${soft.length} files still need splitting`
    : `file size: every file is at most ${LIMIT} lines`,
);
