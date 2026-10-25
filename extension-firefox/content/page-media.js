/*
  Reading the media out of a page.

  A media element can name its own file, but a player that streams through Media
  Source Extensions only holds a blob: URL, which means nothing outside the tab.
  Those players still fetch the real file, so the requests the page makes are
  read as well: an address with an itag, or a plain media file. A manifest for
  HLS or DASH is still not a file the engine can mux, so it is not offered. The
  content script decides when to call these; only the capture store is state.
*/

/** How many direct addresses one video may offer. */
const BDM_SOURCES_PER_ELEMENT = 4;

/** How many media a popup lists. */
const BDM_PAGE_MEDIA_LIMIT = 24;

/** How many stream addresses a page remembers. */
const BDM_CAPTURED_LIMIT = 12;

/** The page title, as a name for a stream the URL cannot name. */
function bdmPageName() {
  // YouTube appends its own name to every video title.
  const title = String(document.title || "").replace(/ - YouTube$/, "").trim();
  return title ? title.slice(0, 90) : "Video";
}

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

/* ------------------------------------------------- the requests a player made */

/** Streams the page fetched, in the order they were seen, one per itag. */
const captured = [];
const capturedKeys = new Set();
let watchingRequests = false;

/** The whole-file address of every stream the page asked for. */
function bdmCapturedSources() {
  return captured.map((entry) => entry.url);
}

function bdmRememberMediaRequest(url) {
  if (!bdmIsCapturedMedia(url)) return false;
  const key = bdmMediaKey(url);
  if (capturedKeys.has(key)) return false;

  capturedKeys.add(key);
  captured.push({ url: bdmFullMediaUrl(url), key });
  if (captured.length > BDM_CAPTURED_LIMIT) {
    capturedKeys.delete(captured.shift().key);
  }
  return true;
}

/**
 * Read what the page has already fetched, then follow it as it streams.
 *
 * A range request for a signed address becomes one entry per itag, so the panel
 * offers the file rather than a few megabytes of it.
 */
function bdmWatchMediaRequests(onFound) {
  if (watchingRequests) return;
  watchingRequests = true;

  for (const entry of performance.getEntriesByType("resource")) {
    if (bdmRememberMediaRequest(entry.name) && onFound) onFound();
  }

  if (typeof PerformanceObserver !== "function") return;
  try {
    new PerformanceObserver((list) => {
      let found = false;
      for (const entry of list.getEntries()) {
        if (bdmRememberMediaRequest(entry.name)) found = true;
      }
      if (found && onFound) onFound();
    }).observe({ entryTypes: ["resource"] });
  } catch {
    // A browser that cannot watch resources keeps what it seeded above.
  }
}

/* ------------------------------------------------------------- the page list */

/** Every downloadable media address on the page, for the popup list. */
function bdmPageMedia() {
  const items = [];
  const seen = new Set();

  const add = (url, kind) => {
    const key = bdmMediaKey(url);
    if (!url || seen.has(key) || items.length >= BDM_PAGE_MEDIA_LIMIT) return;
    seen.add(key);
    const page = bdmPageName();
    items.push({
      url,
      kind,
      label: bdmMediaLabel(url),
      name: bdmMediaName(url, page),
      file: bdmMediaFileName(url, page),
    });
  };

  for (const element of document.querySelectorAll("video, audio")) {
    const kind = element.tagName === "AUDIO" ? "audio" : "video";
    for (const url of bdmSourcesFor(element)) add(url, kind);
  }

  // A player that streams through MSE names no file of its own; the requests
  // it made do.
  for (const url of bdmCapturedSources()) {
    add(url, bdmMediaKind(url) === "audio" ? "audio" : "video");
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

/** How many downloadable media this page offers. */
function bdmMediaCount() {
  const keys = new Set(capturedKeys);
  for (const element of document.querySelectorAll("video, audio")) {
    for (const url of bdmSourcesFor(element)) keys.add(bdmMediaKey(url));
  }
  return Math.min(99, keys.size);
}
