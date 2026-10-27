// Print the CHANGELOG.md section for a version (default: the one in
// package.json). The release workflow feeds this into the GitHub release
// body and latest.json's notes, so the release page and the in-app "What's
// new" always agree.
import { existsSync, readFileSync } from "node:fs";

const wanted = (process.argv[2] ?? "").trim().replace(/^v/, "");
const version =
  wanted || JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;

const fallback =
  "Download the installer from this release. Installed copies update automatically.";

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
const end = lines.findIndex((line, i) => i > start && line.startsWith("## ["));
const notes = lines.slice(start, end === -1 ? undefined : end).join("\n").trim();
process.stdout.write(notes || fallback);
