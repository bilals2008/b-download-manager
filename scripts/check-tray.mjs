// Self-check for the tray icon model: the three styles, the URL list rules,
// and the two places the same list is written down. Node strips the types, so
// the check runs against the same module the window imports.
//
// Run it with `pnpm run check:tray`.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const rootDir = fileURLToPath(new URL("../", import.meta.url));
const {
  DEFAULT_TRAY_PREFERENCES,
  DEFAULT_TRAY_STYLE,
  DEFAULT_URL_PROTOCOLS,
  MAX_URL_PROTOCOLS,
  TRAY_STYLES,
  cleanUrlProtocol,
  normalizeUrlProtocols,
  trayPreferencesFrom,
} = await import("../src/lib/tray.ts");

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

/* --------------------------------------------------------------- styles */

is(
  TRAY_STYLES.map((style) => style.value),
  ["default", "3d", "classic"],
  "the menu offers the three styles, in the order the reference product lists them",
);
is(
  TRAY_STYLES.map((style) => style.label),
  ["Default style", "3D style", "Classic style"],
  "each style is named the way the menu names it",
);
is(DEFAULT_TRAY_STYLE, "default", "the application starts on its own mark");
is(
  DEFAULT_TRAY_PREFERENCES.urlProtocols,
  [...DEFAULT_URL_PROTOCOLS],
  "the defaults the menu opens on are the defaults the backend saves",
);

/* ------------------------------------------------------------- the assets */

for (const style of TRAY_STYLES) {
  for (const surface of ["dark", "light"]) {
    const file = path.join(
      rootDir,
      "src-tauri",
      "icons",
      `tray-${style.value}-on-${surface}.png`,
    );
    is(
      existsSync(file),
      true,
      `the ${style.value} icon for a ${surface} taskbar was drawn by make-icons.py`,
    );
  }
}

/* ------------------------------------------------------------ url list */

is(cleanUrlProtocol("HTTPS"), "https", "a protocol is stored lower-case");
is(cleanUrlProtocol("  rtsp  "), "rtsp", "the spaces around a typed protocol are not part of it");
is(cleanUrlProtocol("mms://"), "mms", "a protocol typed with its slashes is stored without them");
is(cleanUrlProtocol("web+http"), "web+http", "a scheme may hold a plus");
is(cleanUrlProtocol("ftp.archive"), "ftp.archive", "a scheme may hold a dot");
is(cleanUrlProtocol("x-trace"), "x-trace", "a scheme may hold a hyphen");

is(cleanUrlProtocol("1http"), "", "a scheme cannot start with a digit");
is(cleanUrlProtocol("http://example.org/a.zip"), "", "an address is not a protocol");
is(cleanUrlProtocol(""), "", "nothing is not a protocol");
is(cleanUrlProtocol("a".repeat(17)), "", "a scheme longer than 16 characters is refused");

is(
  normalizeUrlProtocols(["https", "https", " mms ", "not a protocol"]),
  ["https", "mms"],
  "repeats and junk leave the list",
);
is(
  normalizeUrlProtocols([]),
  [...DEFAULT_URL_PROTOCOLS],
  "an empty list falls back, because a watch with nothing to match offers nothing",
);
is(
  normalizeUrlProtocols(["ftp"]).length,
  1,
  "a list the reader trimmed to one protocol stays one protocol",
);
is(
  normalizeUrlProtocols(Array.from({ length: 60 }, (_, index) => `a${index}`)).length,
  MAX_URL_PROTOCOLS,
  "the list stops at the length the backend keeps",
);

/* ------------------------------------------------------ parity with Rust */

const rust = readFileSync(
  path.join(rootDir, "src-tauri", "src", "prefs.rs"),
  "utf8",
);
const declared = rust.match(
  /DEFAULT_URL_PROTOCOLS: &\[&str\] = &\[([^\]]*)\]/,
);
is(declared !== null, true, "the backend declares the protocols it starts with");
is(
  [...declared[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]),
  [...DEFAULT_URL_PROTOCOLS],
  "the window and the backend start with the same protocols, in the same order",
);
is(
  /pub const MAX_URL_PROTOCOLS: usize = (\d+);/.exec(rust)[1],
  String(MAX_URL_PROTOCOLS),
  "the window and the backend keep the same number of protocols",
);

/* ------------------------------------------------------- what is loaded */

is(
  trayPreferencesFrom({
    trayStyle: "3d",
    trayHidden: true,
    trayDarkMode: true,
    urlProtocols: ["HTTPS", "https", "mms"],
  }),
  { style: "3d", hidden: true, darkMode: true, urlProtocols: ["https", "mms"] },
  "the saved file is read into the choices the menu shows",
);
is(
  trayPreferencesFrom({
    trayStyle: "classic",
    trayHidden: false,
    trayDarkMode: false,
    urlProtocols: [],
  }).urlProtocols,
  [...DEFAULT_URL_PROTOCOLS],
  "an empty list on disk reads back as the defaults, the way the backend stores it",
);

console.log(`tray: ${checks} checks pass`);
