/**
 * Image galleries: a page of pictures, into one folder.
 *
 * The page is read with the application's own fetch, so the encrypted DNS
 * and the chosen network adapter apply. Each image is an ordinary download.
 */

import { invoke } from "@/lib/ipc";
import type { PageMedia } from "@/lib/ipc-page-media";

/** What the dialog shows before anything starts. */
export interface GalleryInspect {
  /** Whether the page reads as a gallery of images. */
  isGallery: boolean;
  /** The folder the files would land in. */
  folder: string;
  /** How many images the page carries. */
  count: number;
  /** Whether there were more than the cap, so the dialog says so. */
  truncated: boolean;
  /** The video and the sound the page names. The page is read once for both
   * questions, so this arrives with the gallery's answer. */
  media: PageMedia;
}

/** What became of the gallery download. */
export interface GalleryReport {
  /** The folder the files went into. */
  folder: string;
  /** How many images were started. */
  started: number;
  /** How many were refused before the engine was asked. */
  refused: number;
  /** Whether there were more than the cap, so the dialog says so. */
  truncated: boolean;
}

/** Ask the page what it holds. Nothing is downloaded. */
export function galleryInspect(
  url: string,
  referer?: string,
): Promise<GalleryInspect> {
  return invoke<GalleryInspect>("gallery_inspect", { url, referer: referer ?? null });
}

/** Download every image of the gallery into a folder named after the page. */
export function galleryDownload(
  url: string,
  folder?: string,
  referer?: string,
): Promise<GalleryReport> {
  return invoke<GalleryReport>("gallery_download", {
    url,
    folder: folder ?? null,
    referer: referer ?? null,
  });
}
