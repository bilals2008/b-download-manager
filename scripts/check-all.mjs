// Runs every self-check script in one pass. Each check-*.mjs is standalone
// (node:assert, no framework), so this runner just spawns them all, collects
// every failure instead of stopping at the first, and prints one summary line
// per script. CI runs this job; a green run means every rule below shipped
// intact.
//
// Two scripts deliberately stay out of this list:
//   bridge-probe.mjs  drives a RUNNING desktop app over the real loopback
//                     bridge; CI has no app to talk to.
//   fetch-ffmpeg.mjs  the sidecar it checks for is gitignored, so the gate
//                     fails on every fresh checkout by design; the package and
//                     release jobs run fetch-ffmpeg.mjs itself instead.
//
// check-extension.mjs transitively runs check-links.mjs and check-overlay.mjs,
// so those two files execute through it, not beside it.
//
// The list is explicit on purpose: adding a check script must be a deliberate
// edit here, never a glob that silently picks up a half-finished file.

import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = dirname(fileURLToPath(import.meta.url));

const CHECKS = [
  "check-palette",
  "check-fonts",
  "check-tooltip",
  "check-selection",
  "check-find",
  "check-progress",
  "check-add-dialog",
  "check-categories",
  "check-properties",
  "check-recovery",
  "check-toolbars",
  "check-tray",
  "check-settings",
  "check-updates",
  "check-tips",
  "check-share",
  "check-grabber",
  "check-extension",
  "check-whats-new",
  "check-release-integrity",
];

const results = [];

for (const name of CHECKS) {
  const { status } = spawnSync(process.execPath, [resolve(scriptsDir, `${name}.mjs`)], {
    stdio: "inherit",
  });
  results.push({ name, passed: status === 0 });
}

const failures = results.filter((result) => !result.passed);

console.log("\ncheck:all summary");
for (const { name, passed } of results) {
  console.log(`  ${passed ? "pass" : "FAIL"}  ${name}`);
}

if (failures.length > 0) {
  console.error(`\n${failures.length} of ${results.length} checks failed.`);
  process.exit(1);
}
console.log(`\nAll ${results.length} checks passed.`);
