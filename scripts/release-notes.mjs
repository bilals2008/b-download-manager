// Print the reader-facing notes for a version (default: the one in
// package.json). WHATS_NEW.md is the single source of truth for these words:
// the release workflow feeds them into the GitHub release body and
// latest.json's notes, so the release page and the in-app What's New popup
// agree. CHANGELOG.md is only the fallback for a tag that predates the
// friendly notes file.
//
// Pass --strict to make a missing section an error instead of a warning. The
// release workflow uses it after check-release-integrity.mjs has already
// proven the section exists, so a release body can never quietly become a
// generic sentence.
import { existsSync, readFileSync } from "node:fs";

const strict = process.argv.includes("--strict");
const wanted = (process.argv.slice(2).find((argument) => !argument.startsWith("--")) ?? "")
  .trim()
  .replace(/^v/, "");
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

function missing(section, message) {
  if (strict) {
    console.error(`release-notes: ${section} has no notes for v${version}: ${message}`);
    process.exit(1);
  }
  console.error(`::warning::${section} has no readable section for v${version}`);
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
  missing("WHATS_NEW.md", 'add a "## x.y.z" section with short bullet lines');
} else if (strict) {
  console.error("release-notes: WHATS_NEW.md is missing");
  process.exit(1);
}

const changelog = new URL("../CHANGELOG.md", import.meta.url);
if (!existsSync(changelog)) {
  if (strict) {
    console.error("release-notes: CHANGELOG.md is missing");
    process.exit(1);
  }
  process.stdout.write(fallback);
  process.exit(0);
}

const heading = `## [${version}]`;
const lines = readFileSync(changelog, "utf8").split(/\r?\n/);
const start = lines.findIndex((line) => line.startsWith(heading));
if (start === -1) {
  missing("CHANGELOG.md", `expected a "${heading}" heading`);
  process.stdout.write(fallback);
  process.exit(0);
}
const notes = bodyBetween(lines, start, /^## \[/);
if (!notes && strict) {
  console.error(`release-notes: the ${heading} section is empty`);
  process.exit(1);
}
process.stdout.write(notes || fallback);
