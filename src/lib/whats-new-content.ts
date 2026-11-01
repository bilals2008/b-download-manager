/**
 * The What's New file, folded into the build.
 *
 * A raw import keeps the notes inside the signed bundle: there is no request
 * at runtime, nothing to fail offline, and nothing outside the app can change
 * the words between builds. The text is rendered as text, never as HTML.
 */
import whatsNewMarkdown from "../../WHATS_NEW.md?raw";

import {
  latestRelease,
  parseWhatsNew,
  releaseForVersion,
  type WhatsNewRelease,
} from "@/lib/whats-new";

export const WHATS_NEW_RELEASES = parseWhatsNew(whatsNewMarkdown);

/**
 * The notes to show for this build.
 *
 * A section written for the running version wins. When a build goes out
 * before its notes are added, the newest section is shown instead of an empty
 * window, and the version is printed on the page so nothing is claimed to be
 * this build that is not.
 */
export function whatsNewFor(version: string | null): WhatsNewRelease | null {
  return (
    releaseForVersion(WHATS_NEW_RELEASES, version) ??
    latestRelease(WHATS_NEW_RELEASES)
  );
}

/** The notes written for exactly this version, or null when there are none. */
export function dueReleaseFor(version: string | null): WhatsNewRelease | null {
  return releaseForVersion(WHATS_NEW_RELEASES, version);
}
