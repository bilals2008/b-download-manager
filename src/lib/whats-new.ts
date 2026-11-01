/**
 * What's New: the friendly notes bundled with each release, and the one fact
 * that keeps the automatic popup from repeating.
 *
 * The content lives in `WHATS_NEW.md` at the project root, not in the
 * technical changelog. This module only knows the shape of that file; the
 * build-time import lives in `whats-new-content.ts`, so the parser can be
 * checked by Node without a bundler.
 */

export interface WhatsNewSection {
  /** The friendly group name, or null for a list without one. */
  title: string | null;
  items: string[];
}

export interface WhatsNewRelease {
  version: string;
  /** The day the release shipped, as it is written in the file. */
  date: string | null;
  /** The opening sentences before the first group, if the release has any. */
  intro: string[];
  sections: WhatsNewSection[];
}

/** Where the last version whose notes were offered is remembered. */
export const WHATS_NEW_STORAGE_KEY = "idm-clone.whats-new.v1";

/**
 * The choices an installation leaves behind before this feature existed.
 *
 * A machine with any of these has used the app before, even though it has no
 * remembered version yet. A truly fresh profile has none of them before this
 * check, so it is the only case that must stay quiet.
 */
const HISTORY_KEYS = [
  "idm-clone.settings.v1",
  "idm-clone.categories.v1",
  "idm-clone.grabber.v1",
  "idm-clone.last-folder.v1",
  "idm-clone.tips.v1",
  "idm-clone.toolbar-size.v1",
  "idm-clone.toolbar.v1",
] as const;

const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

/** `## 0.2.0 — 31 October 2026`, with or without the date. */
const VERSION_HEADING =
  /^##\s+v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)\s*(?:[—–-]\s*(.+))?$/;

/** What the popup should do with the notes it has for this build. */
export type WhatsNewDecision = "show" | "none" | "baseline";

/**
 * Read the notes out of the markdown file.
 *
 * The file is written by hand for a reader, not for a markdown renderer: a
 * version heading, optional friendly groups, and one short line per change.
 * Everything else is skipped, and the releases come back newest first.
 */
export function parseWhatsNew(markdown: string): WhatsNewRelease[] {
  const releases: WhatsNewRelease[] = [];
  let release: WhatsNewRelease | null = null;
  let section: WhatsNewSection | null = null;
  let inComment = false;

  for (const raw of markdown.replace(/^\uFEFF/, "").split(/\r?\n/)) {
    const line = raw.trim();

    if (inComment) {
      if (line.includes("-->")) {
        inComment = false;
      }
      continue;
    }
    if (line.startsWith("<!--")) {
      if (!line.includes("-->")) {
        inComment = true;
      }
      continue;
    }
    if (!line) {
      continue;
    }
    // The file uses flat groups only. An indented line is a note to a human
    // editor or a continuation, never a new release or highlight.
    if (/^\s+/.test(raw)) {
      continue;
    }

    const heading = VERSION_HEADING.exec(line);
    if (heading) {
      const date = (heading[2] ?? "").trim();
      release = {
        version: heading[1],
        date: date || null,
        intro: [],
        sections: [],
      };
      releases.push(release);
      section = null;
      continue;
    }

    // The document title, or a heading this file does not use. Neither starts
    // a release, so anything under it stays out of the notes.
    if (/^#{1,2}\s+/.test(line)) {
      release = null;
      section = null;
      continue;
    }

    if (!release) {
      continue;
    }

    const group = /^###\s+(.+)$/.exec(line);
    if (group) {
      section = { title: plainText(group[1]), items: [] };
      release.sections.push(section);
      continue;
    }

    const bullet = /^[-*+]\s+(.+)$/.exec(line);
    if (bullet) {
      const text = plainText(bullet[1]);
      if (!text) {
        continue;
      }
      if (!section) {
        section = { title: null, items: [] };
        release.sections.push(section);
      }
      section.items.push(text);
      continue;
    }

    if (/^([-*_])\1{2,}$/.test(line) || /^[>|]/.test(line)) {
      continue;
    }

    const text = plainText(line);
    if (text && release.sections.length === 0 && release.intro.length < 3) {
      release.intro.push(text);
    }
  }

  const cleaned = releases
    .map((item) => ({
      ...item,
      sections: item.sections.filter((group) => group.items.length > 0),
    }))
    .filter((item) => item.intro.length > 0 || item.sections.length > 0);

  return cleaned.sort((a, b) => compareVersions(b.version, a.version));
}

/** The newest release the file carries, or null when it carries none. */
export function latestRelease(
  releases: readonly WhatsNewRelease[],
): WhatsNewRelease | null {
  if (releases.length === 0) {
    return null;
  }
  return releases.reduce((newest, item) =>
    compareVersions(item.version, newest.version) > 0 ? item : newest,
  );
}

/** The notes written for exactly this version, or null. */
export function releaseForVersion(
  releases: readonly WhatsNewRelease[],
  version: string | null | undefined,
): WhatsNewRelease | null {
  const wanted = normalizeVersion(version);
  if (!wanted) {
    return null;
  }
  return releases.find((item) => item.version === wanted) ?? null;
}

/**
 * What to do on this launch.
 *
 * No remembered version means a first install, or an install from before this
 * feature existed. Both are recorded as the starting point and left alone:
 * there is no earlier version to describe, so a popup would be misleading.
 * A version older than the remembered one is a downgrade and is recorded the
 * same quiet way.
 */
export function whatsNewDecision(
  currentVersion: string | null | undefined,
  seenVersion: string | null | undefined,
): WhatsNewDecision {
  const current = normalizeVersion(currentVersion);
  const seen = normalizeVersion(seenVersion);
  if (!current || !seen) {
    return "baseline";
  }
  const order = compareVersions(current, seen);
  if (order === 0) {
    return "none";
  }
  return order > 0 ? "show" : "baseline";
}

/**
 * The startup decision, including the one release that had no version record
 * to compare against.
 *
 * A build from before this feature finds no remembered version on a machine
 * that has already used the app. Treat that as an update to this build: the
 * current release's notes are new to that reader and are offered once. A
 * fresh profile has no saved choices at all, so it only records the starting
 * point and stays quiet.
 */
export function whatsNewStartupDecision(
  currentVersion: string | null | undefined,
  seenVersion: string | null | undefined,
  usedAppBefore: boolean,
): WhatsNewDecision {
  const decision = whatsNewDecision(currentVersion, seenVersion);
  if (
    decision === "baseline" &&
    !normalizeVersion(seenVersion) &&
    usedAppBefore &&
    normalizeVersion(currentVersion)
  ) {
    return "show";
  }
  return decision;
}

/** Whether this machine carries a saved choice from an earlier run. */
export function hasUsedTheAppBefore(): boolean {
  try {
    return HISTORY_KEYS.some(
      (key) => window.localStorage.getItem(key) !== null,
    );
  } catch {
    return false;
  }
}

/** The version whose notes were last offered, or null before any. */
export function loadSeenVersion(): string | null {
  try {
    const raw = window.localStorage.getItem(WHATS_NEW_STORAGE_KEY);
    if (!raw) {
      return null;
    }
    const value = JSON.parse(raw) as { version?: unknown };
    return typeof value?.version === "string"
      ? normalizeVersion(value.version)
      : null;
  } catch {
    return null;
  }
}

/** Remember that this version's notes have been offered. */
export function saveSeenVersion(version: string): void {
  const normalized = normalizeVersion(version);
  if (!normalized) {
    return;
  }
  try {
    window.localStorage.setItem(
      WHATS_NEW_STORAGE_KEY,
      JSON.stringify({ version: normalized }),
    );
  } catch {
    // Storage can be blocked; the popup is still at most one per launch.
  }
}

/** Forget the record, so the next newer version is treated as fresh again. */
export function clearSeenVersion(): void {
  try {
    window.localStorage.removeItem(WHATS_NEW_STORAGE_KEY);
  } catch {
    // Nothing to clear when storage is not there.
  }
}

/** `v0.2.0` and `0.2.0+build` in the one form the file uses. */
export function normalizeVersion(
  value: string | null | undefined,
): string | null {
  const text = (value ?? "").trim().replace(/^v/i, "");
  return VERSION.test(text) ? text : null;
}

/**
 * Semver order, including a prerelease: `1.0.0-rc.1` comes before `1.0.0`.
 * Build metadata is ignored, as it does not change which release is newer.
 */
export function compareVersions(left: string, right: string): number {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (!a || !b) {
    return 0;
  }
  for (let index = 0; index < 3; index += 1) {
    if (a.core[index] !== b.core[index]) {
      return a.core[index] < b.core[index] ? -1 : 1;
    }
  }

  if (a.prerelease.length === 0 && b.prerelease.length === 0) {
    return 0;
  }
  if (a.prerelease.length === 0) {
    return 1;
  }
  if (b.prerelease.length === 0) {
    return -1;
  }

  const length = Math.max(a.prerelease.length, b.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const first = a.prerelease[index];
    const second = b.prerelease[index];
    if (first === undefined) {
      return -1;
    }
    if (second === undefined) {
      return 1;
    }
    const firstNumber = /^\d+$/.test(first);
    const secondNumber = /^\d+$/.test(second);
    if (firstNumber && secondNumber) {
      const difference = Number(first) - Number(second);
      if (difference !== 0) {
        return difference < 0 ? -1 : 1;
      }
    } else if (firstNumber !== secondNumber) {
      return firstNumber ? -1 : 1;
    } else if (first !== second) {
      return first < second ? -1 : 1;
    }
  }
  return 0;
}

interface ParsedVersion {
  core: [number, number, number];
  prerelease: string[];
}

function parseVersion(value: string): ParsedVersion | null {
  const normalized = normalizeVersion(value);
  if (!normalized) {
    return null;
  }
  const withoutBuild = normalized.split("+")[0];
  const dash = withoutBuild.indexOf("-");
  const coreText = dash === -1 ? withoutBuild : withoutBuild.slice(0, dash);
  const prereleaseText = dash === -1 ? "" : withoutBuild.slice(dash + 1);
  const parts = coreText.split(".").map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part))) {
    return null;
  }
  return {
    core: [parts[0], parts[1], parts[2]],
    prerelease: prereleaseText ? prereleaseText.split(".") : [],
  };
}

/** One written line reduced to its words, with the markdown kept out. */
function plainText(line: string): string {
  return line
    .replace(/<!--.*?-->/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/__(.+?)__/g, "$1")
    .replace(/<[^>]+>/g, "")
    .replace(/\s+/g, " ")
    .trim();
}
