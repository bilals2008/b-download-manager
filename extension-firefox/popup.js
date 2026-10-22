/* The popup: the state of the application, the takeover switch and the media
   links the open page offers. */

const dot = document.getElementById("dot");
const statusText = document.getElementById("status-text");
const checkButton = document.getElementById("check");
const settingsButton = document.getElementById("settings");
const takeover = document.getElementById("takeover");
const panel = document.getElementById("panel");
const mediaSection = document.getElementById("media");
const mediaList = document.getElementById("media-list");
const mediaEmpty = document.getElementById("media-empty");

document.getElementById("port").textContent = APP.replace(/^https?:\/\//, "");

function setStatus(state, message) {
  dot.dataset.state = state;
  statusText.textContent = message;
}

async function checkApp() {
  setStatus("checking", "Checking the application");
  const running = await appIsRunning();
  setStatus(
    running ? "ok" : "off",
    running ? "Application is running" : "Application is not running",
  );
}

/*
  Runs inside the page, so it must not reach for anything in this file. Only
  links the application can actually fetch are collected: an MSE stream lives
  in a blob: URL that means nothing outside that tab.
*/
function mediaOnPage() {
  const MEDIA = /\.(mp4|m4v|webm|mkv|mov|avi|mp3|m4a|flac|wav|ogg|oga|opus|aac)(\?|#|$)/i;
  const found = new Set();

  const add = (value) => {
    if (!value) return;
    try {
      const url = new URL(value, location.href);
      if (url.protocol === "http:" || url.protocol === "https:") found.add(url.href);
    } catch {
      // A malformed address is not worth a message in the popup.
    }
  };

  for (const element of document.querySelectorAll("video, audio")) {
    add(element.currentSrc);
    add(element.src);
    for (const source of element.querySelectorAll("source")) add(source.src);
  }

  for (const link of document.querySelectorAll("a[href]")) {
    if (MEDIA.test(link.href)) add(link.href);
  }

  return [...found].slice(0, 30);
}

function renderMedia(urls, pageUrl) {
  mediaList.textContent = "";
  mediaSection.hidden = false;
  mediaEmpty.hidden = urls.length > 0;

  for (const url of urls) {
    const row = document.createElement("li");
    row.className = "media-row";

    const name = document.createElement("span");
    name.className = "media-name";
    name.textContent = nameOfLink(url) || url;
    name.title = url;

    const button = document.createElement("button");
    button.type = "button";
    button.className = "ghost";
    button.textContent = "Download";
    button.addEventListener("click", async () => {
      button.disabled = true;
      const sent = await handOver(url, nameOfLink(url), pageUrl);
      setStatus(
        sent ? "ok" : "warn",
        sent ? "Handed to the application" : "The application did not take the link",
      );
      button.disabled = false;
    });

    row.append(name, button);
    mediaList.append(row);
  }
}

async function showMedia() {
  const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.id || !/^https?:/.test(tab.url || "")) return;

  try {
    const frames = await browser.scripting.executeScript({
      target: { tabId: tab.id },
      func: mediaOnPage,
    });
    renderMedia(frames[0]?.result ?? [], tab.url);
  } catch {
    // A page the extension may not read simply has nothing to offer.
    renderMedia([], tab.url);
  }
}

checkButton.addEventListener("click", checkApp);

settingsButton.addEventListener("click", () => {
  browser.runtime.openOptionsPage();
});

function apply(settings) {
  takeover.checked = settings.takeover;
  panel.checked = settings.panel;
}

takeover.addEventListener("change", async () => {
  const settings = await loadSettings();
  await saveSettings({ ...settings, takeover: takeover.checked });
});

panel.addEventListener("change", async () => {
  const settings = await loadSettings();
  await saveSettings({ ...settings, panel: panel.checked });
});

loadSettings().then(apply);

/* The settings page may be open in another tab. */
browser.storage.onChanged.addListener(async (_changes, area) => {
  if (area !== "local") return;
  apply(await loadSettings());
});

checkApp();
showMedia();
