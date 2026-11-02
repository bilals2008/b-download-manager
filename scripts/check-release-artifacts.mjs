// Verifies a finished set of release artifacts: the installers exist, every
// signature is genuine, and latest.json points at those exact files.
//
// Runs in two places:
//   ci.yml package-linux   against the local bundle folder (installs only,
//                          no latest.json exists there yet)
//   release.yml            against the draft release's assets, with --tag and
//                          --expect all, before a human publishes it
//
// The signature check is the real thing, not a shape check: the minisign
// public key baked into tauri.conf.json is decoded to an ed25519 key and the
// signature is verified over the BLAKE2b-512 digest of the artifact. A single
// flipped byte must make it fail — check-release-artifacts.mjs does that on a
// copy in its own self-test.
//
// Usage:
//   node scripts/check-release-artifacts.mjs --dir <folder> [--platform all]
//                                            [--tag vX.Y.Z] [--expect all]
//                                            [--pubkey <file-or-base64>]

import assert from "node:assert/strict";
import {
  createHash,
  createPublicKey,
  verify as verifySignature,
} from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
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

function argument(name, fallback = null) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? fallback : (process.argv[index + 1] ?? "").trim();
}

const directory = resolve(argument("dir", ""));
const platform = argument("platform", "all");
const tag = argument("tag", null);
const expectAll = process.argv.includes("--expect");

assert.ok(existsSync(directory), `--dir ${directory} does not exist`);
assert.ok(["windows", "linux", "macos", "all"].includes(platform), `unknown platform ${platform}`);

const files = readdirSync(directory);
const find = (pattern) => files.filter((file) => pattern.test(file));

/* ------------------------------------------------------------ public key */

const conf = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
// --pubkey lets the CI bundle job verify a throwaway key it generated for this
// build; a real release always uses the trust root baked into the app.
const pubkeyArgument = argument("pubkey", null);
const encodedKey = pubkeyArgument
  ? existsSync(pubkeyArgument)
    ? readFileSync(pubkeyArgument, "utf8").trim()
    : pubkeyArgument.trim()
  : conf.plugins.updater.pubkey;
const publicKeyFile = Buffer.from(encodedKey, "base64").toString("utf8");
const publicKeyBody = Buffer.from(publicKeyFile.split(/\r?\n/)[1] ?? "", "base64");
assert.equal(publicKeyBody.length, 42, "the updater public key is not a 42-byte minisign key");
const keyId = publicKeyBody.subarray(2, 10).reverse().toString("hex").toUpperCase();
const publicKey = createPublicKey({
  key: Buffer.concat([
    // The DER prefix of an Ed25519 SubjectPublicKeyInfo around a raw 32-byte key.
    Buffer.from("302a300506032b6570032100", "hex"),
    publicKeyBody.subarray(10, 42),
  ]),
  format: "der",
  type: "spki",
});

/**
 * Decode the .sig asset. Tauri stores the whole minisign signature file as
 * base64, so unwrap one layer when the text is not already a comment line.
 */
function signatureText(text) {
  const trimmed = text.trim();
  return trimmed.startsWith("untrusted comment:")
    ? trimmed
    : Buffer.from(trimmed, "base64").toString("utf8");
}

/** Verify one signature (from a file or latest.json) over one artifact. */
function verifyArtifact(label, signature, artifactPath) {
  const lines = signatureText(signature).split(/\r?\n/);
  assert.ok(lines[0]?.startsWith("untrusted comment:"), `${label}: the signature has no comment line`);
  const raw = Buffer.from(lines[1] ?? "", "base64");
  assert.equal(raw.length, 74, `${label}: the minisign signature is not 74 bytes`);
  assert.match(
    raw.subarray(0, 2).toString("ascii"),
    /^E[Dd]$/,
    `${label}: the signature is not EdDSA`,
  );
  const signatureKeyId = raw.subarray(2, 10).reverse().toString("hex").toUpperCase();
  assert.equal(signatureKeyId, keyId, `${label}: signed by key ${signatureKeyId}, not ${keyId}`);
  const body = readFileSync(artifactPath);
  const digest = createHash("blake2b512").update(body).digest();
  assert.ok(
    verifySignature(null, digest, publicKey, raw.subarray(10, 74)),
    `${label}: the signature does not match the file`,
  );
  return body;
}

/* --------------------------------------------------------------- files */

/** Every artifact needs its .sig, and every .sig must name that artifact. */
function signatureFor(artifact) {
  const signaturePath = join(directory, `${artifact}.sig`);
  assert.ok(existsSync(signaturePath), `${artifact}.sig is missing`);
  return readFileSync(signaturePath, "utf8");
}

function expectOne(label, pattern) {
  const matches = find(pattern);
  assert.equal(matches.length, 1, `${label}: expected one ${pattern}, found ${matches.join(", ") || "none"}`);
  return matches[0];
}

const platforms = {
  windows: () => {
    const installer = expectOne("windows", /_x64-setup\.exe$/);
    check(`windows ${installer}`, () =>
      verifyArtifact(installer, signatureFor(installer), join(directory, installer)),
    );
  },
  linux: () => {
    for (const [label, pattern] of [
      ["linux deb", /\.deb$/],
      ["linux AppImage", /\.AppImage$/],
    ]) {
      const artifact = expectOne(label, pattern);
      check(`${label} ${artifact}`, () =>
        verifyArtifact(artifact, signatureFor(artifact), join(directory, artifact)),
      );
    }
  },
  macos: () => {
    expectOne("macos dmg", /\.dmg$/);
    // The .dmg is the installer a person downloads; the updater payload is the
    // app tarball, so both have to exist (a dmg-only build updates nobody).
    const updater = expectOne("macos updater", /\.app\.tar\.gz$/);
    check(`macos ${updater}`, () =>
      verifyArtifact(updater, signatureFor(updater), join(directory, updater)),
    );
  },
};

for (const name of platform === "all" ? Object.keys(platforms) : [platform]) {
  check(`${name} artifacts`, platforms[name]);
}

/* ----------------------------------------------------------- latest.json */

/** Whether a latest.json platform key belongs to the platform under test. */
function belongsToPlatform(name) {
  if (platform === "all") return true;
  if (platform === "macos") return name.startsWith("darwin");
  return name.startsWith(platform);
}

/**
 * The artifact a latest.json entry updates from. The trusted comment's file
 * name shows the product name with spaces, so it cannot be matched against
 * the asset name; the platform key is the reliable link.
 */
function artifactForPlatformKey(name) {
  if (name.startsWith("darwin")) return expectOne(name, /\.app\.tar\.gz$/);
  if (name.startsWith("windows")) return expectOne(name, /_x64-setup\.exe$/);
  if (name.endsWith("-deb")) return expectOne(name, /\.deb$/);
  return expectOne(name, /\.AppImage$/);
}

const latestPath = join(directory, "latest.json");
if (existsSync(latestPath)) {
  check("latest.json", () => {
    const latest = JSON.parse(readFileSync(latestPath, "utf8"));
    if (tag) {
      assert.equal(latest.version, tag.replace(/^v/, ""), `latest.json says ${latest.version}`);
    }
    const entries = Object.entries(latest.platforms ?? {});
    assert.ok(entries.length > 0, "latest.json has no platform entries");
    for (const [name, entry] of entries.filter(([name]) => belongsToPlatform(name))) {
      assert.ok(
        typeof entry.url === "string" && /^https:\/\//.test(entry.url),
        `${name}: url is not https`,
      );
      assert.ok(entry.signature, `${name}: no signature`);
      const artifact = artifactForPlatformKey(name);
      verifyArtifact(`${name} (${artifact})`, entry.signature, join(directory, artifact));
    }
    if (platform === "all") {
      for (const required of ["windows", "linux", "macos"]) {
        const prefix = required === "macos" ? "darwin" : required;
        const present = entries.some(([name]) => name.startsWith(prefix));
        assert.ok(present, `latest.json has no ${required} entry`);
      }
    }
  });
} else if (tag) {
  check("latest.json", () => {
    throw new Error("latest.json is missing from a release");
  });
}

/* -------------------------------------------------------------- verdict */

if (failures.length > 0) {
  console.error(`check-release-artifacts: ${failures.length} problem(s) found in ${directory}`);
  for (const failure of failures) {
    console.error(`  - ${failure}`);
  }
  process.exit(1);
}

console.log(
  `check-release-artifacts: ${platform}${tag ? ` ${tag}` : ""} verified against key ${keyId}`,
);
