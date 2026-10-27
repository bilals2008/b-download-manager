// Write one version everywhere it is recorded.
//
// package.json is the source of truth; this copies its version into
// tauri.conf.json, src-tauri/Cargo.toml and src-tauri/Cargo.lock so no file
// can drift. Use the release script rather than calling this by hand.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pkgPath = resolve(root, "package.json");
const confPath = resolve(root, "src-tauri/tauri.conf.json");
const cargoPath = resolve(root, "src-tauri/Cargo.toml");
const lockPath = resolve(root, "src-tauri/Cargo.lock");

export function currentVersion() {
  return JSON.parse(readFileSync(pkgPath, "utf8")).version;
}

export function bump(version, type) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) throw new Error(`version is not semver: ${version}`);
  let [major, minor, patch] = match.slice(1).map(Number);
  if (type === "major") {
    major += 1;
    minor = 0;
    patch = 0;
  } else if (type === "minor") {
    minor += 1;
    patch = 0;
  } else if (type === "patch") {
    patch += 1;
  } else {
    throw new Error(`unknown bump type: ${type}`);
  }
  return `${major}.${minor}.${patch}`;
}

/** Write `next` into every file that records the version. */
export function syncVersion(next) {
  if (!/^\d+\.\d+\.\d+$/.test(next)) {
    throw new Error(`not a plain x.y.z version: ${next}`);
  }

  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  pkg.version = next;
  writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);

  const conf = JSON.parse(readFileSync(confPath, "utf8"));
  conf.version = next;
  writeFileSync(confPath, `${JSON.stringify(conf, null, 2)}\n`);

  // Only the app crate carries the release version; the engine crates keep
  // their own. The version line directly under `[package]` is the crate's.
  const cargo = readFileSync(cargoPath, "utf8");
  const header = cargo.indexOf("[package]");
  if (header === -1) throw new Error("src-tauri/Cargo.toml has no [package] table");
  const rest = cargo.slice(header);
  const replaced = rest.replace(/version *= *"[^"]*"/, `version = "${next}"`);
  if (replaced === rest) {
    throw new Error("the crate version line in src-tauri/Cargo.toml was not found");
  }
  writeFileSync(cargoPath, cargo.slice(0, header) + replaced);

  // `\r?` because a Windows checkout has CRLF line endings.
  const lock = readFileSync(lockPath, "utf8").replace(
    /(\[\[package\]\]\r?\nname = "my-idm-clone"\r?\nversion = ")[^"]*(")/,
    `$1${next}$2`,
  );
  if (!lock.includes(`name = "my-idm-clone"`)) {
    throw new Error("the app package was not found in src-tauri/Cargo.lock");
  }
  writeFileSync(lockPath, lock);

  return next;
}

// Run directly: node scripts/bump-version.mjs <patch|minor|major|x.y.z>
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const arg = (process.argv[2] ?? "").trim().replace(/^v/, "");
  if (!arg) {
    console.error("usage: node scripts/bump-version.mjs <patch|minor|major|x.y.z>");
    process.exit(1);
  }
  const current = currentVersion();
  const next = /^\d+\.\d+\.\d+$/.test(arg) ? arg : bump(current, arg);
  if (next === current) {
    console.error(`version is already ${current}`);
    process.exit(1);
  }
  console.log(syncVersion(next));
}
