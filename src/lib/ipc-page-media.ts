/**
 * A page pasted into the app: the media it names, and the one way to take it.
 *
 * The quality panel is the only video list, so a page that plays one opens it
 * with `pageMediaOpen`; the files the markup names are ordinary downloads and
 * go through the grabber's hand-over, so they queue, resume and categorise
 * like every other file.
 */

import { invoke } from "@/lib/ipc";

/** One media address a page named. */
export interface FoundMedia {
  /** The absolute address of the file or playlist. */
  url: string;
  /** The name the address gives, cleaned for every system. */
  name: string;
  /** `video` and `audio` are ordinary files the hand-over saves. */
  kind: "video" | "audio";
}

/** What one page holds. */
export interface PageMedia {
  /** The address the markup came from, after any redirect. */
  page: string;
  /** Whether the page reads as one a video player resolves. */
  video: boolean;
  /** The media the markup names, videos first, then sound. */
  files: FoundMedia[];
  /** Whether the page named more files than the list carries. */
  truncated: boolean;
}

/** Open a page or a manifest in the quality panel; answers its job. */
export function pageMediaOpen(
  url: string,
  referer?: string,
  title?: string,
  /** Draw the list in the caller, without opening the quality window. */
  inline?: boolean,
): Promise<number> {
  return invoke<number>("page_media_open", {
    url,
    referer: referer ?? null,
    title: title ?? null,
    inline: inline ?? null,
  });
}
