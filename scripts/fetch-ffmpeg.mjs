#!/usr/bin/env node
/**
 * Places the LGPL ffmpeg the media path joins tracks with.
 *
 * Tauri bundles an `externalBin` from `src-tauri/binaries/` and the file has
 * to carry the target triple in its name. The binary is never committed: this
 * script downloads the pinned build, checks its SHA-256, and copies the one
 * executable the app runs (`ffprobe` and `ffplay` are not used).
 *
 *   node scripts/fetch-ffmpeg.mjs          # place it for this machine
 *   node scripts/fetch-ffmpeg.mjs --check  # exit 1 when it is not there
 *
 * The version, URLs and hashes are pinned so a release is reproducible; when
 * GitHub rotates the build the checksum fails loudly instead of shipping
 * something else. Licensing and redistribution notes: docs/FFMPEG.md.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const BINARIES = join(ROOT, "src-tauri", "binaries");

/** The pinned build: FFmpeg-Builds autobuild-2026-10-31-12-59, LGPL, static. */
const BUILDS = {
  "x86_64-pc-windows-msvc": {
    url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-10-31-12-59/ffmpeg-N-128227-g4811625b5f-win64-lgpl.zip",
    sha256: "6550f7abc950128fbb0069b4552f2972baccf5fbd819f2fd0abbe4ceb8d48734",
    archive: "ffmpeg-win64-lgpl.zip",
    executable: "ffmpeg.exe",
  },
  "x86_64-unknown-linux-gnu": {
    url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-10-31-12-59/ffmpeg-N-128227-g4811625b5f-linux64-lgpl.tar.xz",
    sha256: "f7ce163eca978dc3ab761e6c288f5f764ec4326d17c926c8d4f75ed95e77ee4b",
    archive: "ffmpeg-linux64-lgpl.tar.xz",
    executable: "ffmpeg",
  },
};

function targetTriple() {
  if (process.platform === "win32" && process.arch === "x64") {
    return "x86_64-pc-windows-msvc";
  }
  if (process.platform === "linux" && process.arch === "x64") {
    return "x86_64-unknown-linux-gnu";
  }
  return null;
}

function sidecarPath(triple) {
  return join(BINARIES, `ffmpeg-${triple}${triple.includes("windows") ? ".exe" : ""}`);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function fail(message) {
  console.error(`ffmpeg: ${message}`);
  process.exit(1);
}

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.error) {
    fail(`${command} could not run: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`${command} exited with ${result.status}`);
  }
}

function download(url, destination) {
  console.log(`ffmpeg: downloading ${url}`);
  run("curl", ["-fsSL", "--retry", "3", "-o", destination, url]);
}

function extract(archive, into) {
  if (process.platform === "win32") {
    run("powershell.exe", [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Expand-Archive -LiteralPath '${archive}' -DestinationPath '${into}' -Force`,
    ]);
  } else {
    run("tar", ["-xf", archive, "-C", into]);
  }
}

function main() {
  const check = process.argv.includes("--check");
  const triple = targetTriple();
  if (!triple) {
    fail(
      `no LGPL build is pinned for ${process.platform}/${process.arch}; ` +
        "the media path falls back to an ffmpeg on PATH",
    );
  }
  const build = BUILDS[triple];
  const sidecar = sidecarPath(triple);

  if (check) {
    if (!existsSync(sidecar)) {
      fail(`missing ${sidecar}; run \`pnpm ffmpeg:fetch\` before a release build`);
    }
    console.log(`ffmpeg: ${sidecar} is in place (${statSync(sidecar).size} bytes)`);
    return;
  }

  const cache = join(tmpdir(), "bdm-ffmpeg");
  const archive = join(cache, build.archive);
  mkdirSync(cache, { recursive: true });

  if (!existsSync(archive) || sha256(archive) !== build.sha256) {
    download(build.url, archive);
  }
  const digest = sha256(archive);
  if (digest !== build.sha256) {
    fail(`the downloaded build is not the pinned one (sha256 ${digest})`);
  }

  const extracted = join(cache, `unpacked-${triple}`);
  rmSync(extracted, { recursive: true, force: true });
  mkdirSync(extracted, { recursive: true });
  extract(archive, extracted);

  // The archive keeps its files under a build-name folder, so the executable
  // is found by name rather than by a path the build could change.
  const member = readdirSync(extracted, { recursive: true })
    .map((entry) => join(extracted, entry.toString()))
    .find(
      (entry) =>
        entry.endsWith(`/${build.executable}`) || entry.endsWith(`\\${build.executable}`),
    );
  if (!member) {
    fail(`the archive did not contain ${build.executable}`);
  }
  mkdirSync(BINARIES, { recursive: true });
  writeFileSync(sidecar, readFileSync(member));
  if (process.platform !== "win32") {
    run("chmod", ["755", sidecar]);
  }
  console.log(`ffmpeg: placed ${sidecar} (${statSync(sidecar).size} bytes, lgpl)`);
}

main();
