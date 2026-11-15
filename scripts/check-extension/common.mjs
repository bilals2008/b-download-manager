// Shared helpers and fixtures for the extension self-checks.
//
// These are the pieces more than one check module needs: the extension folder
// paths, the parsed manifest, and the small pure helpers. Moved verbatim from
// scripts/check-extension.mjs.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";

export const rootDir = fileURLToPath(new URL("../../extension-firefox/", import.meta.url));
export const rootUrl = pathToFileURL(rootDir + path.sep);
export const read = (file) => readFileSync(new URL(file, rootUrl), "utf8");
export const exists = (file) => existsSync(new URL(file, rootUrl));

export const manifest = JSON.parse(read("manifest.json"));

/** Width and height from a PNG's IHDR chunk. */
export function pngSize(buffer) {
  assert.equal(buffer.subarray(1, 4).toString("ascii"), "PNG", "not a PNG file");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

/** The work the background page kicks off without being awaited. */
export async function until(predicate, tries = 60) {
  for (let index = 0; index < tries; index += 1) {
    if (predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  return false;
}

// A cleanup timer in the takeover path waits 30 s; the sandbox does not need
// to keep the check alive for it.
export const setTimeoutShim = (fn, delay, ...args) =>
  delay >= 10_000 ? 0 : setTimeout(fn, delay, ...args);