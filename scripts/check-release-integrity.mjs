// The release contract. Everything that must agree before a tag may be built
// or published lives here: the version in every recorded file, the updater
// key and endpoint, the installer configuration, the build wiring, and the
// loopback bridge constants that the Rust app, the Firefox extension and the
// Chrome extension each declare separately.
//
// Read-only and network-free, so it runs on every pull request through
// check-all.mjs and again on the tag in release.yml (with --tag). Every
// problem is collected and printed together; the exit code says whether any
// were found. Run it with `pnpm run check:release`.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

function fail(message) {
  failures.push(message);
}

function check(label, action) {
  try {
    action();
  } catch (error) {
    fail(`${label}: ${error.message}`);
  }
}

function read(path) {
  return readFileSync(resolve(root, path), "utf8");
}

function readJson(path) {
  return JSON.parse(read(path));
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const tagArgument = (() => {
  const index = process.argv.indexOf("--tag");
  return index === -1 ? null : (process.argv[index + 1] ?? "").trim();
})();

/* ------------------------------------------------------------- versions */

const pkg = readJson("package.json");
const conf = readJson("src-tauri/tauri.conf.json");

const cargoToml = read("src-tauri/Cargo.toml");
const cargoPackage = cargoToml.slice(cargoToml.indexOf("[package]"));
const cargoVersion = /version *= *"([^"]+)"/.exec(cargoPackage)?.[1];

const lock = read("Cargo.lock");
const lockVersion = new RegExp(
  `\\[\\[package\\]\\]\\r?\\nname = "my-idm-clone"\\r?\\nversion = "([^"]+)"`,
).exec(lock)?.[1];

const readme = read("README.md");
const badgeVersion = /badge\/version-(\d+\.\d+\.\d+)-/.exec(readme)?.[1];

check("version agreement", () => {
  const sources = {
    "package.json": pkg.version,
    "src-tauri/tauri.conf.json": conf.version,
    "src-tauri/Cargo.toml": cargoVersion,
    "Cargo.lock": lockVersion,
    "README.md badge": badgeVersion,
  };
  const missing = Object.entries(sources)
    .filter(([, value]) => !value)
    .map(([name]) => name);
  if (missing.length > 0) {
    throw new Error(`the version could not be read from ${missing.join(", ")}`);
  }
  const distinct = new Set(Object.values(sources));
  if (distinct.size !== 1) {
    const detail = Object.entries(sources)
      .map(([name, value]) => `${name}=${value}`)
      .join(", ");
    throw new Error(`the recorded versions disagree (${detail})`);
  }
});

check("version format", () => {
  assert.match(pkg.version, /^\d+\.\d+\.\d+$/, `package.json version "${pkg.version}" is not x.y.z`);
});

/* ------------------------------------------------------------------ tag */

if (tagArgument !== null) {
  check("tag agreement", () => {
    assert.match(tagArgument, /^v\d+\.\d+\.\d+$/, `"--tag ${tagArgument}" is not vX.Y.Z`);
    assert.equal(
      tagArgument,
      `v${pkg.version}`,
      `the tag "${tagArgument}" does not match version ${pkg.version}`,
    );
  });
}

/* ---------------------------------------------------------------- notes */

/** The WHATS_NEW.md body for a version, or an empty string. */
function whatsNewBody(version) {
  const heading = new RegExp(`^##\\s+v?${escapeRegExp(version)}(?:\\s|$)`);
  const lines = read("WHATS_NEW.md").split(/\r?\n/);
  const start = lines.findIndex((line) => heading.test(line));
  if (start === -1) return "";
  const end = lines.findIndex((line, index) => index > start && /^##\s+/.test(line));
  return lines.slice(start + 1, end === -1 ? undefined : end).join("\n").trim();
}

check("release notes", () => {
  const body = whatsNewBody(pkg.version);
  assert.ok(body.length > 0, `WHATS_NEW.md has no "## ${pkg.version}" section`);
  assert.match(
    body,
    /^[-*+]\s+\S/m,
    `the "## ${pkg.version}" section in WHATS_NEW.md has no bullet lines for users`,
  );
});

check("changelog section", () => {
  const heading = new RegExp(`^##\\s+\\[?v?${escapeRegExp(pkg.version)}\\]?(\\s|$)`, "m");
  assert.match(read("CHANGELOG.md"), heading, `CHANGELOG.md has no ${pkg.version} section`);
});

/* -------------------------------------------------------------- updater */

check("updater key", () => {
  const encoded = conf.plugins?.updater?.pubkey;
  assert.ok(typeof encoded === "string" && encoded.length > 0, "plugins.updater.pubkey is empty");
  const decoded = Buffer.from(encoded, "base64").toString("utf8");
  const comment = /^untrusted comment: minisign public key: ([0-9A-F]+)\r?$/m.exec(decoded);
  assert.ok(comment, "the pubkey is not a base64 minisign public key file");
  const inner = decoded.split(/\r?\n/)[1]?.trim() ?? "";
  const raw = Buffer.from(inner, "base64");
  assert.equal(raw.length, 42, "the minisign public key body is not 42 bytes");
  assert.equal(raw.subarray(0, 2).toString("utf8"), "Ed", "the minisign key is not EdDSA");
  const keyId = raw.subarray(2, 10).reverse().toString("hex").toUpperCase();
  assert.equal(
    keyId,
    comment[1],
    `the pubkey comment says ${comment[1]} but the key body says ${keyId}`,
  );
});

check("updater endpoint", () => {
  const endpoints = conf.plugins?.updater?.endpoints;
  assert.ok(Array.isArray(endpoints) && endpoints.length === 1, "there must be exactly one endpoint");
  const match = /^https:\/\/github\.com\/([^/]+)\/([^/]+)\/releases\/latest\/download\/latest\.json$/.exec(
    endpoints[0],
  );
  assert.ok(match, `unexpected endpoint ${endpoints[0]}`);
  const [, owner, repo] = match;
  const repository = `GITHUB_REPOSITORY` in process.env ? process.env.GITHUB_REPOSITORY : null;
  if (repository) {
    assert.equal(
      `${owner}/${repo}`,
      repository,
      `the endpoint points at ${owner}/${repo}, not this repository (${repository})`,
    );
  }
  const site = read("site/src/lib/releases.ts");
  const siteRepo = /export const REPO = "([^"]+)"/.exec(site)?.[1];
  if (siteRepo) {
    assert.equal(`${owner}/${repo}`, siteRepo, `the site points at ${siteRepo}, not the app`);
  }
});

/* --------------------------------------------------------------- bundle */

check("bundle configuration", () => {
  const bundle = conf.bundle ?? {};
  assert.equal(bundle.active, true, "bundle.active must be true");
  assert.equal(bundle.createUpdaterArtifacts, true, "createUpdaterArtifacts must be true");
  assert.equal(typeof conf.productName, "string", "productName is missing");
  assert.match(conf.identifier ?? "", /^[a-z0-9.]+$/, "the bundle identifier must be a dotted name");
  assert.equal(
    bundle.windows?.nsis?.installMode,
    "currentUser",
    "the NSIS installer must stay per-user",
  );
  assert.ok(bundle.windows?.webviewInstallMode?.type, "webviewInstallMode is missing");
  for (const icon of bundle.icon ?? []) {
    assert.ok(existsSync(resolve(root, "src-tauri", icon)), `bundle icon ${icon} does not exist`);
  }
  // The installer is branded from these files; a missing one silently drops
  // the branding instead of failing the build.
  for (const key of ["installerIcon", "uninstallerIcon", "headerImage", "sidebarImage"]) {
    const path = bundle.windows?.nsis?.[key];
    assert.ok(typeof path === "string" && path, `nsis.${key} is missing`);
    assert.ok(
      existsSync(resolve(root, "src-tauri", path)),
      `nsis.${key} ${path} does not exist`,
    );
  }
  assert.equal(
    conf.plugins?.updater?.windows?.installMode,
    "passive",
    "in-app updates must stay page-skipping (passive) so they reuse the install folder",
  );
});

/* ---------------------------------------------------------- build wiring */

check("build wiring", () => {
  assert.equal(conf.build?.frontendDist, "../dist", "frontendDist must be ../dist");
  assert.equal(conf.build?.beforeBuildCommand, "pnpm build", "beforeBuildCommand must be pnpm build");
  const devUrlPort = /:(\d+)\/?$/.exec(conf.build?.devUrl ?? "")?.[1];
  const vitePort = /port:\s*(\d+)/.exec(read("vite.config.ts"))?.[1];
  assert.ok(devUrlPort, "devUrl has no port");
  assert.equal(devUrlPort, vitePort, `devUrl uses :${devUrlPort} but vite serves :${vitePort}`);
});

/* ---------------------------------------------------------- bridge links */

const bridgeMod = read("src-tauri/src/bridge/mod.rs");
const bridgeAuth = read("src-tauri/src/bridge/auth.rs");
const firefoxBridge = read("extension-firefox/lib/bridge.js");
const chromeWorker = read("browser-extension/background.js");
const firefoxManifest = readJson("extension-firefox/manifest.json");
const chromeManifest = readJson("browser-extension/manifest.json");

check("bridge port", () => {
  const rustPort = /pub const PORT: u16 = (\d+);/.exec(bridgeMod)?.[1];
  assert.ok(rustPort, "PORT is not declared in src-tauri/src/bridge/mod.rs");
  const firefoxPort = /base: "http:\/\/127\.0\.0\.1:(\d+)"/.exec(firefoxBridge)?.[1];
  const chromePort = /const BRIDGE = "http:\/\/127\.0\.0\.1:(\d+)"/.exec(chromeWorker)?.[1];
  assert.equal(firefoxPort, rustPort, `the Firefox extension uses port ${firefoxPort}`);
  assert.equal(chromePort, rustPort, `the Chrome extension uses port ${chromePort}`);
  for (const [name, manifest] of [
    ["Firefox", firefoxManifest],
    ["Chrome", chromeManifest],
  ]) {
    const hosts = manifest.host_permissions ?? [];
    assert.ok(
      hosts.includes(`http://127.0.0.1:${rustPort}/*`),
      `the ${name} manifest does not allow the bridge port`,
    );
  }
});

check("bridge API version", () => {
  const rustApi = /pub const API_VERSION: u32 = (\d+);/.exec(bridgeMod)?.[1];
  const firefoxApi = /api: (\d+)/.exec(firefoxBridge)?.[1];
  assert.ok(rustApi, "API_VERSION is not declared in src-tauri/src/bridge/mod.rs");
  assert.equal(firefoxApi, rustApi, `the Firefox extension expects API ${firefoxApi}`);
});

check("bridge client header", () => {
  const rustClient = /pub const CLIENT_VALUE: &str = "([^"]+)";/.exec(bridgeAuth)?.[1];
  const firefoxClient = /client: "([^"]+)"/.exec(firefoxBridge)?.[1];
  const chromeClient = /const CLIENT = "([^"]+)"/.exec(chromeWorker)?.[1];
  assert.ok(rustClient, "CLIENT_VALUE is not declared in src-tauri/src/bridge/auth.rs");
  assert.equal(firefoxClient, rustClient, `the Firefox extension sends client ${firefoxClient}`);
  assert.equal(chromeClient, rustClient, `the Chrome extension sends client ${chromeClient}`);
  for (const [name, text] of [
    ["Firefox", firefoxBridge],
    ["Chrome", chromeWorker],
  ]) {
    assert.match(text, /X-BDM-Client/i, `the ${name} extension does not send X-BDM-Client`);
  }
});

/* ----------------------------------------------------- workspace layout */

check("workspace layout", () => {
  assert.ok(existsSync(resolve(root, "Cargo.lock")), "the workspace Cargo.lock is missing");
  assert.ok(
    !existsSync(resolve(root, "src-tauri/Cargo.lock")),
    "src-tauri/Cargo.lock exists again; the workspace has one root lockfile",
  );
  for (const manifest of ["crates/idm-core/Cargo.toml", "crates/idm-cli/Cargo.toml"]) {
    const text = read(manifest);
    const version = /^\[package\]\r?\n(?:[^[]*?\r?\n)?version = "([^"]+)"/m.exec(text)?.[1];
    assert.match(version ?? "", /^\d+\.\d+\.\d+$/, `${manifest} has no parseable version`);
  }
});

/* ------------------------------------------------------------- verdict */

if (failures.length > 0) {
  console.error(`check-release-integrity: ${failures.length} problem(s) found`);
  for (const failure of failures) {
    console.error(`  - ${failure}`);
  }
  process.exit(1);
}

const tagNote = tagArgument === null ? "" : ` (tag ${tagArgument})`;
console.log(`check-release-integrity: ${pkg.version}${tagNote}, contracts agree`);
