/*
  The part of the extension that watches the browser.

  Three things hand a link to the application:

  - the right click menu on a link, a video, an audio element or an image;
  - the floating button the content script draws on a video;
  - a download the browser starts, which the application takes over the way
    IDM's advanced integration does. The settings decide whether a particular
    download is taken: the switch, the file types and the sites left alone.

  This page is also the only context that talks to the loopback bridge. The
  popup and the content script send messages, so the connection handling and
  the error text live in one place.
*/

const BDM_BADGE_BACKGROUND = "#c8ff00";
const BDM_BADGE_TEXT = "#022c1e";

/** The media count of each tab, so a short badge flash can restore it. */
const mediaCounts = new Map();
/** Downloads already handed over, so a repeated event cannot send twice. */
const handedOver = new Set();

/* ------------------------------------------------------------------ menus */

/*
  Menus outlive the event page, so they are built once per install and once
  per browser start. Clearing first keeps a reload from failing on a duplicate
  id.
*/
async function createMenus() {
  await browser.contextMenus.removeAll();
  browser.contextMenus.create({
    id: "bdm-download",
    title: "Download with B Download Manager",
    contexts: ["link", "video", "audio", "image"],
  });
  browser.contextMenus.create({
    id: "bdm-separator",
    type: "separator",
    contexts: ["link", "video", "audio", "image", "page"],
  });
  browser.contextMenus.create({
    id: "bdm-open",
    title: "Open B Download Manager",
    contexts: ["all"],
  });
  browser.contextMenus.create({
    id: "bdm-settings",
    title: "Extension settings",
    contexts: ["all"],
  });
}

browser.runtime.onInstalled.addListener(() => {
  void createMenus();
  announce();
});
browser.runtime.onStartup.addListener(() => {
  void createMenus();
  announce();
});

/**
 * Tell the application the browser is here. A failed ping is normal: the
 * application may not be running, and nothing else depends on it.
 */
function announce() {
  void bdmBridgePing().catch(() => {});
}

/* --------------------------------------------------------------- statuses */

function setBadge(tabId, count) {
  if (typeof tabId !== "number") return;
  mediaCounts.set(tabId, count);
  const text = count > 9 ? "9+" : count > 0 ? String(count) : "";
  browser.action.setBadgeText({ tabId, text }).catch(() => {});
}

/** A check or a warning over the toolbar button for a moment. */
let flashTimer = 0;
function flash(tabId, text) {
  if (typeof tabId !== "number") return;
  browser.action.setBadgeText({ tabId, text }).catch(() => {});
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => {
    setBadge(tabId, mediaCounts.get(tabId) || 0);
  }, 1800);
}

browser.action.setBadgeBackgroundColor({ color: BDM_BADGE_BACKGROUND }).catch(() => {});
browser.action.setBadgeTextColor?.({ color: BDM_BADGE_TEXT })?.catch(() => {});

/* The count belongs to the page that reported it, so it is cleared when that
   tab starts loading another one. */
browser.tabs.onUpdated.addListener((tabId, change) => {
  if (change.status === "loading") setBadge(tabId, 0);
});
browser.tabs.onRemoved.addListener((tabId) => {
  mediaCounts.delete(tabId);
});

/* ---------------------------------------------------------- hand-overs */

/*
  The capture rules. While the application runs, its Settings page owns the
  hand-over switch, the ask switch, the file types and the site list; the
  extension's own copy is the fallback for when the application is closed. The
  overlay and the theme stay the extension's own.
*/
async function bdmEffectiveSettings() {
  const local = await bdmLoadSettings();
  try {
    const policy = await bdmBridgeCapturePolicy();
    if (!policy || typeof policy !== "object") return local;
    return bdmNormalizeSettings({
      ...local,
      intercept: policy.capture === true,
      ask: policy.ask === true,
      types: Array.isArray(policy.types) ? policy.types : local.types,
      skipSites: Array.isArray(policy.skipSites) ? policy.skipSites : local.skipSites,
    });
  } catch {
    return local;
  }
}

/** Send one link to the app. The settings decide whether it asks first. */
async function sendLink(url, name, referer) {
  if (!bdmIsHttpUrl(url) && !/^ftp:\/\//i.test(url)) {
    return { ok: false, error: { kind: "refused", message: "This link is not a download." } };
  }
  const settings = await bdmEffectiveSettings();
  const mode = settings.ask ? "ask" : "direct";
  const result = await bdmBridgeSend({
    url,
    name: name || bdmNameFromUrl(url),
    referer: referer || "",
    mode,
  });
  return { ok: true, result };
}

browser.contextMenus.onClicked.addListener(async (info) => {
  const tabId = info.tab?.id;

  if (info.menuItemId === "bdm-open") {
    try {
      await bdmBridgeFocus();
    } catch {
      // A closed app cannot be brought forward; the popup is where the user
      // sees why it is closed.
    }
    return;
  }

  if (info.menuItemId === "bdm-settings") {
    await browser.runtime.openOptionsPage();
    return;
  }

  if (info.menuItemId !== "bdm-download") return;

  const url = info.linkUrl || info.srcUrl;
  const referer = info.pageUrl || "";
  if (!url) return;

  try {
    // A menu click is a choice of its own, so the skip rules do not apply.
    const sent = await sendLink(url, "", referer);
    flash(tabId, sent.ok ? "✓" : "!");
  } catch {
    flash(tabId, "!");
  }
});

/*
  The browser started a download. Hand it over before the browser copy is
  stopped: if the application is closed, or does not take the link, the file
  stays in the downloads of the browser instead of being lost.
*/
browser.downloads.onCreated.addListener(async (item) => {
  if (!item || handedOver.has(item.id)) return;
  handedOver.add(item.id);
  setTimeout(() => handedOver.delete(item.id), 30000);

  const settings = await bdmEffectiveSettings();
  if (!bdmShouldIntercept(settings, item.url, item.referrer || "")) return;

  const name = bdmFileNameOfPath(item.filename) || bdmNameFromUrl(item.url);

  try {
    await bdmBridgeSend({
      url: item.url,
      name,
      referer: item.referrer || "",
      mode: settings.ask ? "ask" : "direct",
    });
  } catch {
    // The app did not take it. The browser download goes on untouched.
    return;
  }

  await browser.downloads.cancel(item.id).catch(() => {});
  await browser.downloads.erase({ id: item.id }).catch(() => {});
});

/* --------------------------------------------------------------- messages */

function bridgeFailure(error) {
  return {
    ok: false,
    error: {
      kind: error?.kind || "unknown",
      message: error?.message || "The app could not be reached.",
      status: error?.status || 0,
      code: error?.code || "",
    },
  };
}

async function handleMessage(message, sender) {
  switch (message.type) {
    case "bdm:status": {
      try {
        const data = await bdmBridgePing();
        return { ok: true, version: data.version, api: data.api };
      } catch (error) {
        return bridgeFailure(error);
      }
    }

    case "bdm:list": {
      try {
        const data = await bdmBridgeList();
        return { ok: true, ...data };
      } catch (error) {
        return bridgeFailure(error);
      }
    }

    case "bdm:send": {
      const payload = message.payload || {};
      if (!bdmIsHttpUrl(payload.url) && !/^ftp:\/\//i.test(payload.url || "")) {
        return { ok: false, error: { kind: "refused", message: "This link is not a download." } };
      }
      try {
        const settings = await bdmEffectiveSettings();
        const result = await bdmBridgeSend({
          url: payload.url,
          name: payload.name || bdmNameFromUrl(payload.url),
          referer: payload.referer || "",
          mode: message.mode === "ask" || settings.ask ? "ask" : "direct",
        });
        return { ok: true, result };
      } catch (error) {
        return bridgeFailure(error);
      }
    }

    case "bdm:control": {
      const actions = ["pause", "resume", "cancel", "show"];
      if (!actions.includes(message.action)) {
        return { ok: false, error: { kind: "refused", message: "Unknown action." } };
      }
      const id = Number(message.id);
      if (!Number.isInteger(id) || id <= 0) {
        return { ok: false, error: { kind: "refused", message: "Unknown download." } };
      }
      try {
        const data = await bdmBridgeControl(id, message.action);
        return { ok: true, ...data };
      } catch (error) {
        return bridgeFailure(error);
      }
    }

    case "bdm:focus": {
      try {
        await bdmBridgeFocus();
        return { ok: true };
      } catch (error) {
        return bridgeFailure(error);
      }
    }

    case "bdm:media-count": {
      const count = Math.max(0, Math.min(99, Number(message.count) || 0));
      if (sender.tab?.id !== undefined) setBadge(sender.tab.id, count);
      return { ok: true };
    }

    case "bdm:open-settings": {
      await browser.runtime.openOptionsPage();
      return { ok: true };
    }

    default:
      return undefined;
  }
}

browser.runtime.onMessage.addListener((message, sender) => {
  if (!message || typeof message.type !== "string" || !message.type.startsWith("bdm:")) {
    return undefined;
  }
  return handleMessage(message, sender).catch((error) => bridgeFailure(error));
});
