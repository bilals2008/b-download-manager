/*
  The part of the extension that watches the browser.

  Two things hand a link to the application:

  - the right click menu on a link, a video or an audio element;
  - a download the browser starts, which the application takes over the way
    IDM's advanced integration does. The settings decide whether a particular
    download is taken: the switch, the file types and the sites left alone.
*/

// Menus outlive the event page, so they are built once per install and once
// per browser start. Clearing first keeps a reload from failing on a
// duplicate id.
async function createMenus() {
  await browser.contextMenus.removeAll();
  browser.contextMenus.create({
    id: "idm-download",
    title: "Download with B Download Manager",
    contexts: ["link", "video", "audio"],
  });
}

browser.runtime.onInstalled.addListener(createMenus);
browser.runtime.onStartup.addListener(createMenus);

browser.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "idm-download") return;
  const url = info.linkUrl || info.srcUrl;
  // A menu click is a choice of its own, so the skip rules do not apply.
  if (!url || !LINK.test(url)) return;
  handOver(url, nameOfLink(url), info.pageUrl);
});

browser.downloads.onCreated.addListener(async (item) => {
  const settings = await loadSettings();
  if (!shouldTakeOver(settings, item.url, item.referrer || "")) return;
  if (!(await appIsRunning())) return;

  // Hand over first: if the application does not take it, the browser keeps
  // its own download instead of losing the file.
  const accepted = await handOver(
    item.url,
    baseName(item.filename) || nameOfLink(item.url),
    item.referrer || "",
  );
  if (!accepted) return;

  await browser.downloads.cancel(item.id).catch(() => {});
  await browser.downloads.erase({ id: item.id }).catch(() => {});
});

/*
  The page panel. It cannot call the bridge itself, because the request would
  carry the page's origin and the bridge refuses it; the hand-over and the
  badge therefore happen here.
*/

// The accent of the application, used as the badge. The theme file is CSS and
// the badge API takes a colour, so this pair is written out.
const BADGE_BACKGROUND = "#c8ff00";
const BADGE_TEXT = "#022c1e";

browser.action.setBadgeBackgroundColor({ color: BADGE_BACKGROUND }).catch(() => {});
browser.action.setBadgeTextColor?.({ color: BADGE_TEXT })?.catch(() => {});

/** The number on the toolbar button, the count of media the open page offers. */
function setBadge(tabId, count) {
  const text = count > 9 ? "9+" : count > 0 ? String(count) : "";
  browser.action.setBadgeText({ tabId, text }).catch(() => {});
}

browser.runtime.onMessage.addListener((message, sender) => {
  if (message?.type === "download") {
    if (!LINK.test(message.url || "")) return Promise.resolve({ ok: false });
    return handOver(message.url, message.name, message.referer).then((ok) => ({ ok }));
  }

  if (message?.type === "media-count") {
    if (sender.tab?.id !== undefined) setBadge(sender.tab.id, message.count);
  }
});

/* The count belongs to the page that reported it, so it is cleared when that
   tab starts loading another one. */
browser.tabs.onUpdated.addListener((tabId, change) => {
  if (change.status === "loading") setBadge(tabId, 0);
});
