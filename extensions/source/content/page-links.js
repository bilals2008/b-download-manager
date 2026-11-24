/*
  Reading the links a page offers, without freezing it.

  A page can hold thousands of anchors. The scan walks the browser's own
  `document.links` collection, which is a live view rather than a copy, in
  slices of a few milliseconds, and yields to the page between them, so typing
  and scrolling stay smooth while the window fills. It remembers only what it
  offers: a duplicate link is counted and dropped, so one address is never
  listed twice, and both the number of links examined and the number of links
  kept are capped, so a page with a million links costs a bounded amount of
  memory.

  The rules for what a link is live in lib/link-rules.js, so the window shows
  the same classification the scan used. Nothing is sent anywhere here: the
  window asks the user, and the background hands over only what was ticked.
*/

/** The most anchors one scan examines. */
const BDM_LINK_SCAN_ELEMENTS = 6000;

/** The most distinct links one scan keeps, and the window lists. */
const BDM_LINK_SCAN_RESULTS = 400;

/** How long one slice may run before the scan yields to the page. */
const BDM_LINK_SCAN_SLICE_MS = 8;

/** The most anchors one slice walks, so a fast page still yields. */
const BDM_LINK_SCAN_CHUNK = 200;

/** How often the scan reports its progress, in milliseconds. */
const BDM_LINK_PROGRESS_MS = 120;

/** The scan that is current. A newer scan stops the one before it. */
let bdmActiveLinkScan = "";

/** Let the page run before the next slice. */
function bdmLinkYield() {
  const scheduler = globalThis.scheduler;
  if (scheduler && typeof scheduler.yield === "function") {
    return scheduler.yield();
  }
  return new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * Collect the links of this page.
 *
 * `scanId` names this run, so a window that asks again is answered by the
 * newest scan and the older one stops instead of competing with it.
 * `onProgress` is called at most every 120 ms with how far the scan has got.
 */
async function bdmScanPageLinks(scanId, onProgress) {
  const id = String(scanId || "scan");
  bdmActiveLinkScan = id;

  const page = String(location.href || "");
  const collection = document.links;
  const available = Number(collection?.length) || 0;
  const total = Math.min(available, BDM_LINK_SCAN_ELEMENTS);

  const found = new Map();
  const skipped = { self: 0, scheme: 0, stream: 0, unusable: 0 };
  let examined = 0;
  let duplicates = 0;
  let lastProgress = 0;
  let cancelled = false;

  const report = (force) => {
    const now = performance.now();
    if (!force && now - lastProgress < BDM_LINK_PROGRESS_MS) return;
    lastProgress = now;
    onProgress?.({ scanned: examined, total, found: found.size });
  };

  while (examined < total && found.size < BDM_LINK_SCAN_RESULTS) {
    if (bdmActiveLinkScan !== id) {
      cancelled = true;
      break;
    }

    const sliceStart = performance.now();
    let inSlice = 0;
    while (
      examined < total &&
      inSlice < BDM_LINK_SCAN_CHUNK &&
      found.size < BDM_LINK_SCAN_RESULTS
    ) {
      const anchor = collection[examined];
      examined += 1;
      inSlice += 1;

      const answer = bdmLinkClassify(anchor, page);
      if (!answer.ok) {
        skipped[answer.reason] = (skipped[answer.reason] || 0) + 1;
      } else if (found.has(answer.item.key)) {
        duplicates += 1;
      } else {
        found.set(answer.item.key, answer.item);
      }

      if (performance.now() - sliceStart >= BDM_LINK_SCAN_SLICE_MS) break;
    }

    report(false);
    if (examined < total && found.size < BDM_LINK_SCAN_RESULTS) {
      await bdmLinkYield();
    }
  }

  report(true);

  const links = [];
  for (const item of found.values()) {
    links.push({
      url: item.url,
      key: item.key,
      name: item.name,
      extension: item.extension,
      category: item.category,
      kind: item.kind,
      badge: item.badge,
    });
  }

  return {
    ok: true,
    page: {
      url: page,
      host: String(location.hostname || ""),
      title: String(document.title || "").trim().slice(0, 200),
    },
    links,
    stats: {
      scanned: examined,
      total: available,
      found: links.length,
      files: links.filter((item) => item.kind === "file").length,
      pages: links.filter((item) => item.kind === "page").length,
      duplicates,
      cancelled,
      truncated:
        available > BDM_LINK_SCAN_ELEMENTS || found.size >= BDM_LINK_SCAN_RESULTS,
      skipped,
    },
  };
}

browser.runtime.onMessage.addListener((message) => {
  if (!message || message.type !== "bdm:links-scan") return undefined;

  return bdmScanPageLinks(message.scanId, (progress) => {
    // A closed window rejects this; the scan does not need the answer.
    browser.runtime
      .sendMessage({
        type: "bdm:links-progress",
        scanId: message.scanId,
        ...progress,
      })
      .catch(() => {});
  }).catch(() => ({
    ok: false,
    error: "The page could not be read.",
  }));
});
