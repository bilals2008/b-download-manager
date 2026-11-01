// Print the reader-facing notes for a version (default: the one in
// package.json). WHATS_NEW.md is the single source of truth for these words:
// the release workflow feeds them into the GitHub release body and
// latest.json's notes, so the release page and the in-app What's New popup
// agree. CHANGELOG.md is only the fallback for a tag that predates the
// friendly notes file.
import { existsSync, readFileSync } from "node:fs";

const wanted = (process.argv[2] ?? "").trim().replace(/^v/, "");
const version =
  wanted || JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;

const fallback =
  "Download the installer from this release. Installed copies update automatically.";

function escapeVersion(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function bodyBetween(lines, start, endPattern) {
  const end = lines.findIndex((line, index) => index > start && endPattern.test(line));
  return lines.slice(start + 1, end === -1 ? undefined : end).join("\n").trim();
}

const whatsNew = new URL("../WHATS_NEW.md", import.meta.url);
if (existsSync(whatsNew)) {
  const lines = readFileSync(whatsNew, "utf8").split(/\r?\n/);
  const heading = new RegExp(`^##\\s+v?${escapeVersion(version)}(?:\\s|$)`);
  const start = lines.findIndex((line) => heading.test(line));
  if (start !== -1) {
    const notes = bodyBetween(lines, start, /^##\s+/);
    if (notes) {
      process.stdout.write(notes);
      process.exit(0);
    }
  }
  console.error(`::warning::WHATS_NEW.md has no readable section for v${version}`);
}

if (!existsSync("CHANGELOG.md")) {
  process.stdout.write(fallback);
  process.exit(0);
}

const heading = `## [${version}]`;
const lines = readFileSync("CHANGELOG.md", "utf8").split(/\r?\n/);
const start = lines.findIndex((line) => line.startsWith(heading));
if (start === -1) {
  console.error(`::warning::CHANGELOG.md has no ${heading} section`);
  process.stdout.write(fallback);
  process.exit(0);
}
const notes = bodyBetween(lines, start, /^## \[/);
process.stdout.write(notes || fallback);
