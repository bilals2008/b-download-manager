/*
  The part of the extension that talks to my-idm-clone.

  The application listens on the loopback interface, on port 27500, and takes
  a link the same way the add dialog does. Two things hand a link over:

  - the right click menu on a link,
  - a download the browser starts, which the extension takes over, the way
    the "advanced integration" of IDM does.

  The takeover can be turned off in the popup of the extension.
*/

const BRIDGE = "http://127.0.0.1:27500/download";

/** Hand a link to the application. */
async function handOver(url, referer, name) {
  const query = new URLSearchParams({
    url: url,
    name: name || "",
    referer: referer || "",
  });
  try {
    const answer = await fetch(`${BRIDGE}?${query.toString()}`);
    if (!answer.ok) {
      console.warn("my-idm-clone answered", answer.status);
    }
  } catch (error) {
    console.warn("my-idm-clone did not answer", error);
  }
}

/** The file name of a link, the way the application would name it. */
function fileNameOf(url) {
  try {
    const path = new URL(url).pathname;
    const last = path.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : "";
  } catch {
    return "";
  }
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "idm-download",
    title: "Download with B Download Manager",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "idm-download") {
    return;
  }
  handOver(info.linkUrl, info.pageUrl, fileNameOf(info.linkUrl));
});

/* The download the browser starts is taken over by the application. */
chrome.downloads.onCreated.addListener(async (item) => {
  if (!item.url.startsWith("http://") && !item.url.startsWith("https://")) {
    return;
  }
  const settings = await chrome.storage.local.get({ takeover: true });
  if (!settings.takeover) {
    return;
  }
  await chrome.downloads.cancel(item.id);
  handOver(item.url, item.referrer, item.filename);
});
