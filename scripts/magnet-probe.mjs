#!/usr/bin/env node
/*
  An opt-in proof of a magnet download against a real swarm.

  It builds and runs the magnet_probe example of the torrent crate — never
  part of the application bundle, never part of `pnpm verify` or CI — and
  streams what the engine actually saw: status, peers, verified bytes. An
  empty swarm is an honest result (exit 0); only a job that crashes is a
  failure (exit 1).

    pnpm probe:magnet                          # the current Debian netinst
    pnpm probe:magnet -- <url-or-file>         # a torrent of your own
    pnpm probe:magnet -- --seconds 240         # watch longer
*/

import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

console.log("magnet probe: a real job against a real swarm (opt-in, not a CI gate)");

const child = spawn(
  "cargo",
  ["run", "-p", "idm-torrent", "--example", "magnet_probe", "--", ...process.argv.slice(2)],
  {
    cwd: root,
    stdio: "inherit",
    env: {
      ...process.env,
      // Build discipline (AGENTS.md): a one-off verification never reuses an
      // incremental cache, and the caches plus .pdb files are the disk bloat.
      CARGO_INCREMENTAL: "0",
      CARGO_PROFILE_DEV_DEBUG: "0",
    },
  },
);

child.on("error", (error) => {
  console.error(`the probe could not run: ${error.message}`);
  process.exit(1);
});
child.on("exit", (code) => {
  process.exit(code ?? 1);
});