// Make a release: bump, changelog, commit, tag, push.
//
//   pnpm release:patch | pnpm release:minor | pnpm release:major
//   node scripts/release.mjs explicit 0.1.0
//
// The version moves in every recorded place (package.json is the source of
// truth; see bump-version.mjs), the CHANGELOG section for the new version is
// generated from the conventional commits since the last tag, everything is
// committed and tagged vX.Y.Z, and the tag is pushed. The pushed tag is what
// starts .github/workflows/release.yml: it builds the signed Windows
// installer and publishes the GitHub release that latest.json points at, so
// installed copies see the update.
//
// Requirements: a clean worktree, an `origin` remote, and your commit
// subjects following the convention (feat: / fix: / perf: / refactor: ...).

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { bump, currentVersion, syncVersion } from "./bump-version.mjs";
import { commitsInRange, renderSection } from "./changelog-from-commits.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const changelogPath = resolve(root, "CHANGELOG.md");

const git = (args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();

function guard(condition, message) {
  if (!condition) {
    console.error(`release refused: ${message}`);
    process.exit(1);
  }
}

const kind = (process.argv[2] ?? "").trim();
const explicit = (process.argv[3] ?? "").trim().replace(/^v/, "");
guard(
  ["patch", "minor", "major", "explicit"].includes(kind),
  "usage: node scripts/release.mjs <patch|minor|major|explicit x.y.z>",
);
const current = currentVersion();
const next = kind === "explicit" ? explicit : bump(current, kind);
guard(/^\d+\.\d+\.\d+$/.test(next), `not a plain x.y.z version: ${next || "(empty)"}`);
guard(next !== current, `v${current} is already the current version`);
guard(kind !== "major" || Number(current.split(".")[0]) >= 1, "pre-1.0 releases bump minor, not major");

guard(git("status --porcelain") === "", "the worktree has uncommitted changes; commit or stash them first");
const remotes = git("remote");
guard(/\borigin\b/.test(remotes), "no origin remote; create the GitHub repo and `git remote add origin …` first");

const tags = git("tag").split(/\r?\n/).filter(Boolean);
guard(!tags.includes(`v${next}`), `tag v${next} already exists`);

// The commits this release carries: everything since the highest tag, or all
// of history when nothing is tagged yet.
const previous = tags
  .filter((tag) => /^v\d+\.\d+\.\d+$/.test(tag))
  .sort((a, b) => {
    const pa = a.slice(1).split(".").map(Number);
    const pb = b.slice(1).split(".").map(Number);
    return pa[0] - pb[0] || pa[1] - pb[1] || pa[2] - pb[2];
  })
  .at(-1);
const range = previous ? `${previous}..HEAD` : null;
const commits = commitsInRange(range, execFileSync);
guard(commits.length > 0, `there are no commits ${previous ? `since ${previous}` : "in the repository"} to release`);

syncVersion(next);

const date = new Date().toLocaleDateString("en-GB", {
  day: "numeric",
  month: "long",
  year: "numeric",
});
const section = renderSection(next, date, commits);
if (!existsSync(changelogPath)) {
  writeFileSync(
    changelogPath,
    [
      "# Changelog",
      "",
      "All notable changes to B Download Manager will be documented in this file.",
      "",
      "The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),",
      "and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).",
      "",
      "Sections are generated from the conventional commits each release carries;",
      "edit one before its release ships if a bullet needs better words.",
      "",
      "---",
      "",
      section,
    ].join("\n"),
  );
} else {
  const changelog = readFileSync(changelogPath, "utf8");
  const marker = "\n---\n";
  const at = changelog.indexOf(marker);
  guard(at !== -1, "CHANGELOG.md lost its `---` divider; restore it so sections can be inserted");
  writeFileSync(changelogPath, `${changelog.slice(0, at + marker.length)}\n${section}\n${changelog.slice(at + marker.length).replace(/^\n+/, "\n")}`);
}

git(["add", "package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "CHANGELOG.md"]);
git(["commit", "-m", `chore: release v${next} [skip ci]`]);
git(["tag", `v${next}`]);
git(["push", "origin", `HEAD:${git("rev-parse --abbrev-ref HEAD")}`]);
git(["push", "origin", `v${next}`]);

console.log(`v${next} tagged and pushed — watch the Release workflow build it.`);
