// Build a loadable extension folder for one browser.
//
// The scripts and pages are written once, in `source/`. A browser folder holds
// only what that browser needs on top of them: its `manifest.json`, and for
// Chromium the service-worker entry that imports the shared background. The
// build copies the source, overlays the browser folder, and writes the result
// to `dist/<browser>/` — the folder a browser is loaded from, and the folder
// the self-checks read.
//
//   node extensions/build.mjs            all three browsers
//   node extensions/build.mjs firefox    just one
//   node extensions/build.mjs --clean    remove dist/ and stop

import { cpSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(fileURLToPath(import.meta.url));

/** The browsers a folder exists for. Chromium covers Chrome, Edge, Brave,
    Opera, Vivaldi and Arc: they run the same manifest and the same engine. */
export const BROWSERS = ["chromium", "firefox", "safari"];

/** The files a browser folder adds to the shared source. */
function overlays(browser) {
  const dir = path.join(ROOT, browser);
  return readdirSync(dir)
    .filter((name) => !name.endsWith(".md"))
    .map((name) => path.join(dir, name));
}

/**
 * Build one browser's folder and return its path.
 *
 * A rebuild always starts from an empty output folder: a file removed from the
 * source must not survive in a folder a browser is about to load.
 */
export function buildExtension(browser) {
  if (!BROWSERS.includes(browser)) {
    throw new Error(
      `Unknown browser "${browser}"; expected one of ${BROWSERS.join(", ")}.`,
    );
  }

  const out = path.join(ROOT, "dist", browser);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  cpSync(path.join(ROOT, "source"), out, { recursive: true });

  for (const file of overlays(browser)) {
    cpSync(file, path.join(out, path.basename(file)), {
      recursive: statSync(file).isDirectory(),
    });
  }

  return out;
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes("--clean")) {
    rmSync(path.join(ROOT, "dist"), { recursive: true, force: true });
    console.log("extensions: dist removed");
    return;
  }

  const wanted = args.filter((arg) => !arg.startsWith("-"));
  const browsers = wanted.length > 0 ? wanted : BROWSERS;
  for (const browser of browsers) {
    console.log(`extensions: ${browser} -> ${buildExtension(browser)}`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
