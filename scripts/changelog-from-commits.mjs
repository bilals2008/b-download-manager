// Turn conventional commit subjects into a Keep a Changelog section.
//
// The release script calls this so the notes for a version are written once,
// by the person making each commit, and reused everywhere: CHANGELOG.md, the
// GitHub release body, and the "What's new" text the in-app updater shows.
// Commits that do not follow the convention are listed under "Changed" with
// their raw subject rather than dropped, so nothing in the range is invisible.

const GROUPS = [
  { title: "Added", types: ["feat"] },
  { title: "Changed", types: ["refactor", "style"] },
  { title: "Deprecated", types: ["depr"] },
  { title: "Removed", types: ["rm"] },
  { title: "Fixed", types: ["fix"] },
  { title: "Performance", types: ["perf"] },
];

const RECORD_SEP = "\u241F"; // a visible unit separator: safe inside subjects
const FIELD_SEP = "\u241E";

/** `git log` for a range, with subject, body, hash and breaking marker. */
export function commitsInRange(range, execFileSync) {
  const format = [`%H`, `%s`, `%b`, RECORD_SEP].join(FIELD_SEP);
  const args = ["log", "--no-merges", `--format=${format}`];
  if (range) args.push(range);
  const out = execFileSync("git", args, { encoding: "utf8" });
  const commits = [];
  for (const record of out.split(RECORD_SEP)) {
    const trimmed = record.replace(/^\r?\n/, "");
    if (!trimmed.trim()) continue;
    const [hash, subject, body, bangs] = trimmed.split(FIELD_SEP);
    commits.push({
      hash,
      subject: subject.trim(),
      body: (body ?? "").trim(),
      breaking: (bangs ?? "").includes("!") || /BREAKING CHANGE/.test(body ?? ""),
    });
  }
  return commits;
}

/**
 * The markdown lines for one version, newest group first, empty groups
 * dropped. `heading` is the `## [x.y.z] - date` title without its level.
 */
export function renderSection(version, date, commits) {
  const lines = [`## [${version}] - ${date}`, ""];

  const breaking = commits.filter((commit) => commit.breaking);
  if (breaking.length > 0) {
    lines.push("### Breaking changes", "");
    for (const commit of breaking) lines.push(...bullet(commit));
  }

  for (const group of GROUPS) {
    const own = commits.filter(
      (commit) =>
        !commit.breaking && group.types.includes(conventional(commit.subject)?.type),
    );
    if (own.length === 0) continue;
    lines.push(`### ${group.title}`, "");
    for (const commit of own) lines.push(...bullet(commit));
  }

  const recognised = new Set(GROUPS.flatMap((group) => group.types));
  const left = commits.filter(
    (commit) =>
      !commit.breaking &&
      !(conventional(commit.subject)?.type && recognised.has(conventional(commit.subject).type)),
  );
  if (left.length > 0) {
    lines.push("### Changed", "");
    for (const commit of left) lines.push(...bullet(commit));
  }

  return `${lines.join("\n").trimEnd()}\n`;
}

/** `feat(grabber)!: subject` parsed; anything else has no type. */
function conventional(subject) {
  const match = /^(\w+)(\(([^)]*)\))?(!)?:\s+(.+)$/.exec(subject);
  if (!match) return null;
  return { type: match[1].toLowerCase(), scope: match[3] ?? null, text: match[5] };
}

function bullet(commit) {
  const parsed = conventional(commit.subject);
  const text = parsed ? parsed.text : commit.subject;
  const scope = parsed?.scope ? `**${parsed.scope}:** ` : "";
  const lines = [`- ${scope}${text.charAt(0).toUpperCase()}${text.slice(1)}`];
  // A body that reads as a note, not a footer, rides along as a sub bullet.
  const note = commit.body
    .split(/\r?\n/)
    .filter((line) => line.trim() && !/^[A-Z-]+: /.test(line))
    .join(" ")
    .trim();
  if (note && note.toLowerCase() !== text.toLowerCase()) {
    lines.push(`  - ${note.charAt(0).toUpperCase()}${note.slice(1)}`);
  }
  return lines;
}
