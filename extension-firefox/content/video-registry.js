/*
  Finding the videos a page can hand over.

  A page builds its player long after the document has loaded, replaces the
  <video> when it navigates inside the same document, and stacks its own
  controls above the picture. Hover detection therefore cannot rely on the
  element alone:

  - the player's controls take the pointer, so an event over a YouTube player
    usually names a control div rather than the <video>;
  - a node that arrives through a page event is not guaranteed to be an object
    of this content script's realm, so `instanceof HTMLVideoElement` can be
    false for a real video (Firefox keeps the page and the script in separate
    compartments);
  - a video can be replaced without the page ever firing an event the overlay
    listens to.

  This registry keeps the list of video elements, follows the document for the
  new and the removed ones, and answers "which video is at this point". Nothing
  here draws anything: the overlay decides what the answer means.
*/

/** How often the list is reconciled when the document sends no mutation. */
const BDM_VIDEO_SWEEP_MS = 4000;

/** The registry's own state. Started once per document. */
const videoRegistry = {
  videos: new Set(),
  observer: null,
  observed: null,
  sweep: 0,
  frame: 0,
  onChange: null,
  running: false,
};

/**
 * The video an element is, or null.
 *
 * The test reads `localName` instead of asking `instanceof`: a node that came
 * from the page through an event belongs to the page's realm, and its
 * prototype is not the one this script sees.
 */
function bdmIsVideo(node) {
  if (!node || node.nodeType !== 1) return false;
  return String(node.localName || "").toLowerCase() === "video";
}

/** The first video in an event path, or null. */
function bdmVideoInPath(path) {
  if (!path || typeof path.length !== "number") return null;
  for (const node of path) {
    if (bdmIsVideo(node)) return node;
  }
  return null;
}

/** The videos the registry knows about. */
function bdmKnownVideos() {
  return [...videoRegistry.videos].filter((video) => video.isConnected);
}

/** Whether a point is inside a box, in viewport pixels. */
function bdmBoxHas(rect, x, y) {
  return (
    Boolean(rect) &&
    rect.width > 0 &&
    rect.height > 0 &&
    x >= rect.left &&
    x <= rect.right &&
    y >= rect.top &&
    y <= rect.bottom
  );
}

/**
 * The video at a point.
 *
 * The event path answers first. When it names no video the point is compared
 * with the box of every known video, which is what finds a player whose own
 * controls sit above the picture: the pointer is over a control, but it is
 * still over the video's part of the page.
 */
function bdmVideoAtPoint(x, y, path, preferred) {
  const fromPath = bdmVideoInPath(path);
  if (fromPath) return fromPath;
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null;

  // The video the button already follows answers first: on a page with a
  // player and a few previews that is the only box this has to read.
  if (
    bdmIsVideo(preferred) &&
    preferred.isConnected &&
    bdmBoxHas(preferred.getBoundingClientRect(), x, y)
  ) {
    return preferred;
  }

  let found = null;
  let area = 0;
  for (const video of bdmKnownVideos()) {
    const rect = video.getBoundingClientRect();
    if (!bdmBoxHas(rect, x, y)) continue;
    const box = rect.width * rect.height;
    if (box > area) {
      area = box;
      found = video;
    }
  }
  return found;
}

/* ------------------------------------------------------------- following */

/** Reconcile the list with the document. Returns true when it changed. */
function bdmScanVideos() {
  const root = document.documentElement || document.body;
  if (!root) return false;

  const found = new Set();
  if (typeof root.querySelectorAll === "function") {
    for (const video of root.querySelectorAll("video")) found.add(video);
  }

  let changed = found.size !== videoRegistry.videos.size;
  if (!changed) {
    for (const video of found) {
      if (!videoRegistry.videos.has(video)) {
        changed = true;
        break;
      }
    }
  }
  if (!changed) return false;

  videoRegistry.videos = found;
  return true;
}

/** Run one scan soon, once per frame however many mutations arrive. */
function bdmScheduleVideoScan() {
  if (videoRegistry.frame) return;
  const run = () => {
    videoRegistry.frame = 0;
    if (!videoRegistry.running) return;
    if (!bdmScanVideos()) return;
    videoRegistry.onChange?.();
  };

  if (typeof requestAnimationFrame === "function") {
    videoRegistry.frame = requestAnimationFrame(run);
  } else {
    videoRegistry.frame = setTimeout(run, 16);
  }
}

/** Follow the document, and follow a document that is replaced again. */
function bdmObserveDocument() {
  const root = document.documentElement;
  if (!root || typeof MutationObserver !== "function") return;
  if (videoRegistry.observer && videoRegistry.observed === root) return;

  videoRegistry.observer?.disconnect();
  videoRegistry.observer = new MutationObserver(bdmScheduleVideoScan);
  videoRegistry.observer.observe(root, { childList: true, subtree: true });
  videoRegistry.observed = root;
}

/**
 * Start following the document.
 *
 * `onChange` runs whenever the set of video elements changed, which is the
 * moment the overlay has to look again: the video it was showing may be gone,
 * and a new player may be waiting for its button.
 */
function bdmStartVideoRegistry(options = {}) {
  if (videoRegistry.running) return bdmStopVideoRegistry;

  videoRegistry.running = true;
  videoRegistry.onChange = typeof options.onChange === "function" ? options.onChange : null;
  bdmScanVideos();
  bdmObserveDocument();

  videoRegistry.sweep = setInterval(() => {
    // A replaced document takes its observer with it; this puts it back, and
    // catches a player that built itself without a mutation we could see.
    bdmObserveDocument();
    if (!document.hidden) bdmScheduleVideoScan();
  }, BDM_VIDEO_SWEEP_MS);

  return bdmStopVideoRegistry;
}

/** Stop following, and forget every element. */
function bdmStopVideoRegistry() {
  if (!videoRegistry.running) return;
  videoRegistry.running = false;
  videoRegistry.onChange = null;
  videoRegistry.observer?.disconnect();
  videoRegistry.observer = null;
  videoRegistry.observed = null;
  videoRegistry.videos = new Set();
  if (videoRegistry.sweep) clearInterval(videoRegistry.sweep);
  videoRegistry.sweep = 0;
  if (videoRegistry.frame) {
    if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(videoRegistry.frame);
    else clearTimeout(videoRegistry.frame);
    videoRegistry.frame = 0;
  }
}
