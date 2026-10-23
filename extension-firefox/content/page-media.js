/*
  Reading the media out of a page.

  Only addresses the application can fetch are returned. A blob: URL from MSE
  means nothing outside the tab, and a manifest for HLS or DASH is not a file
  the engine can mux yet, so neither is offered. The functions are pure and the
  content script decides when to call them.
*/

/** How many addresses one video may offer. */
const BDM_SOURCES_PER_ELEMENT = 4;

/** How many media a popup lists. */
const BDM_PAGE_MEDIA_LIMIT = 24;

/** The addresses of one video or audio element that can be downloaded. */
function bdmSourcesFor(element) {
  const urls = [];
  if (!element || element.mediaKeys) {
    // A protected element has no address the application may read, and the
    // browser will not hand one over either.
    return urls;
  }

  const add = (value) => {
    if (!value) return;
    try {
      const url = new URL(value, location.href);
      if (!bdmIsDownloadableSource(url.href)) return;
      if (!urls.includes(url.href)) urls.push(url.href);
    } catch {
      // A malformed address is not worth showing.
    }
  };

  add(element.currentSrc);
  add(element.src);
  for (const source of element.querySelectorAll("source")) add(source.src);

  return urls.slice(0, BDM_SOURCES_PER_ELEMENT);
}

/** Every downloadable media address on the page, for the popup list. */
function bdmPageMedia() {
  const items = [];
  const seen = new Set();

  const add = (url, kind) => {
    if (!url || seen.has(url) || items.length >= BDM_PAGE_MEDIA_LIMIT) return;
    seen.add(url);
    items.push({
      url,
      kind,
      label: bdmSourceLabel(url),
      name: bdmNameFromUrl(url) || (kind === "audio" ? "Audio" : "Video"),
    });
  };

  for (const element of document.querySelectorAll("video, audio")) {
    const kind = element.tagName === "AUDIO" ? "audio" : "video";
    for (const url of bdmSourcesFor(element)) add(url, kind);
  }

  // Media links are only read when the popup asks, never on every page
  // mutation, so a long page is not walked on its own.
  if (items.length < BDM_PAGE_MEDIA_LIMIT) {
    for (const link of document.querySelectorAll("a[href]")) {
      if (items.length >= BDM_PAGE_MEDIA_LIMIT) break;
      const href = link.href;
      if (bdmIsMediaLink(href)) add(href, "file");
    }
  }

  return items;
}

/** How many media elements offer a file the application can download. */
function bdmMediaCount() {
  let count = 0;
  for (const element of document.querySelectorAll("video, audio")) {
    if (bdmSourcesFor(element).length) count += 1;
  }
  return Math.min(99, count);
}
