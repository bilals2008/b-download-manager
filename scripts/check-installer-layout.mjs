// The generated NSIS script is the only honest proof that a fresh install
// still offers a folder, that the chosen folder is recorded, and that an
// in-app update returns to it. Tauri's template can change between versions
// without the app noticing, so the release build checks the script it just
// produced.
//
// Run it after `pnpm tauri build --bundles nsis`, or let the Windows release
// job run it. The branding files and the per-user scope are also checked by
// check-release-integrity before anything is built; this checks what NSIS was
// actually asked to compile.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const candidates = [
  process.argv[2],
  "target/release/nsis/x64/installer.nsi",
  "src-tauri/target/release/nsis/x64/installer.nsi",
]
  .filter(Boolean)
  .map((candidate) => resolve(candidate));
const path = candidates.find(existsSync);

assert.ok(
  path,
  `no generated NSIS script in ${candidates.join(", ")}; build the installer first`,
);
const script = readFileSync(path, "utf8");

function defines(name) {
  const match = new RegExp(`^!define ${name} "(.*)"$`, "m").exec(script);
  assert.ok(match, `the generated script has no ${name}`);
  return match[1];
}

assert.equal(defines("INSTALLMODE"), "currentUser", "the installer must stay per-user");

// The directory page is the requirement, not an incidental wizard step.
assert.match(
  script,
  /^\s*!insertmacro MUI_PAGE_DIRECTORY\s*$/m,
  "the generated installer no longer offers the install folder page",
);

// The page is skipped when the updater runs in passive mode, so the update
// never asks again.
assert.match(
  script,
  /!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive\s*\n!insertmacro MUI_PAGE_DIRECTORY/,
  "the directory page no longer skips itself for a passive update",
);

// The chosen folder is written once and restored on every later run.
assert.match(
  script,
  /WriteRegStr SHCTX "\$\{MANUPRODUCTKEY\}" "" \$INSTDIR/,
  "the installer no longer records the chosen install folder",
);
assert.match(
  script,
  /Call RestorePreviousInstallLocation/,
  "the installer no longer restores the previous install folder",
);

for (const name of ["INSTALLERICON", "HEADERIMAGE", "SIDEBARIMAGE"]) {
  assert.ok(defines(name), `the installer was generated without ${name}`);
}

console.log(`installer layout ok: ${path}`);
