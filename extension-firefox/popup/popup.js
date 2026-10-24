/* The popup: connection state, the downloads in flight and their controls. */

const refs = {
  status: document.getElementById("status"),
  statusText: document.getElementById("status-text"),
  retry: document.getElementById("retry"),
  openApp: document.getElementById("open-app"),
  settings: document.getElementById("settings"),
  downloads: document.getElementById("downloads"),
  count: document.getElementById("download-count"),
  empty: document.getElementById("downloads-empty"),
  mediaSection: document.getElementById("media-section"),
  mediaList: document.getElementById("media-list"),
  mediaCount: document.getElementById("media-count"),
};

const POLL_MS = 1000;
const OFFLINE_POLL_MS = 2500;
const MAX_ROWS = 20;
const MAX_MEDIA = 12;

/** One element per download id, so a poll updates instead of rebuilding. */
const rows = new Map();
/** The last sample of each download, for the speed the app does not send. */
const speedSamples = new Map();

let closed = false;
let pollTimer = 0;
let inFlight = false;

/* ---------------------------------------------------------------- helpers */

function message(payload) {
  return browser.runtime
    .sendMessage(payload)
    .catch(() => ({
      ok: false,
      error: { kind: "offline", message: "The extension is restarting." },
    }));
}

function setConnection(state, text) {
  refs.status.dataset.state = state;
  refs.statusText.textContent = text;
  refs.retry.hidden = state !== "offline";
  refs.openApp.hidden = state === "offline" || state === "checking";
}

function connectionText(error) {
  switch (error?.kind) {
    case "timeout":
      return "Not responding";
    case "protocol":
      return "Bridge version mismatch";
    default:
      return "Not running";
  }
}

function actionText(error) {
  if (error?.kind === "offline") return "Not running";
  if (error?.kind === "timeout") return "The app is not responding";
  return error?.message || "The app refused it";
}

function schedule(ms) {
  clearTimeout(pollTimer);
  pollTimer = setTimeout(() => void refresh(), ms);
}

function refreshSoon() {
  clearTimeout(pollTimer);
  pollTimer = setTimeout(() => void refresh(), 60);
}

/* -------------------------------------------------------------- downloads */

function setIcon(button, name) {
  button.replaceChildren(bdmIcon(name, "icon"));
}

function iconButton(name, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "icon-button small";
  button.append(bdmIcon(name, "icon"));
  button.title = label;
  button.setAttribute("aria-label", label);
  return button;
}

function createRow(download) {
  const el = document.createElement("li");
  el.className = "download";
  el.dataset.id = String(download.id);

  const head = document.createElement("div");
  head.className = "download-head";

  const file = document.createElement("button");
  file.type = "button";
  file.className = "file-button";
  file.title = "Open in B Download Manager";

  const actions = document.createElement("div");
  actions.className = "actions";

  const pause = iconButton("pause", "Pause download");
  pause.dataset.op = "pause";
  const cancel = iconButton("close", "Stop download");
  cancel.dataset.op = "cancel";
  actions.append(pause, cancel);
  head.append(file, actions);

  const track = document.createElement("div");
  track.className = "track";
  track.setAttribute("role", "progressbar");
  track.setAttribute("aria-valuemin", "0");
  track.setAttribute("aria-valuemax", "100");
  track.setAttribute("aria-label", "Download progress");
  const fill = document.createElement("span");
  track.append(fill);

  const meta = document.createElement("div");
  meta.className = "meta";
  const pct = document.createElement("span");
  pct.className = "pct";
  const detail = document.createElement("span");
  detail.className = "detail";
  const speed = document.createElement("span");
  speed.className = "speed";
  meta.append(pct, detail, speed);

  el.append(head, track, meta);

  const row = {
    el,
    file,
    pause,
    cancel,
    track,
    fill,
    pct,
    detail,
    speed,
    id: download.id,
    busy: false,
    cancelTimer: 0,
  };

  file.addEventListener("click", () => void showInApp(row));
  pause.addEventListener("click", () => void togglePause(row));
  cancel.addEventListener("click", () => onCancel(row));

  return row;
}

function updateRow(row, download) {
  const name = bdmFileNameOfPath(download.dest) || bdmNameFromUrl(download.url) || "Download";
  row.file.textContent = name;
  row.file.title = `${download.dest || name} — open in B Download Manager`;
  row.el.dataset.status = download.status;

  const paused = download.status === "paused";
  const pauseLabel = paused ? "Resume download" : "Pause download";
  row.pause.dataset.op = paused ? "resume" : "pause";
  row.pause.title = pauseLabel;
  row.pause.setAttribute("aria-label", pauseLabel);
  setIcon(row.pause, paused ? "play" : "pause");

  const total = Number(download.total) || 0;
  const done = Number(download.downloaded) || 0;
  const percent = bdmPercentOf(download);
  const indeterminate = total <= 0 && download.status === "running";

  row.track.dataset.indeterminate = indeterminate ? "true" : "false";
  if (indeterminate) {
    row.track.removeAttribute("aria-valuenow");
    row.fill.style.width = "";
  } else {
    row.track.setAttribute("aria-valuenow", String(Math.round(percent)));
    row.fill.style.width = `${Math.min(100, Math.max(0, percent))}%`;
  }

  row.pct.hidden = indeterminate;
  row.pct.textContent = `${Math.round(percent)}%`;
  row.detail.textContent =
    total > 0
      ? `${bdmFormatSize(done)} of ${bdmFormatSize(total)}`
      : bdmFormatSize(done);

  const speed = sampleSpeed(download);
  if (download.status === "paused") {
    row.speed.textContent = "Paused";
  } else if (download.status === "queued") {
    row.speed.textContent = "Queued";
  } else {
    row.speed.textContent = speed > 0 ? bdmFormatSpeed(speed) : "Starting";
  }
}

function sampleSpeed(download) {
  const now = performance.now();
  const id = Number(download.id);
  const done = Number(download.downloaded) || 0;
  const previous = speedSamples.get(id);

  if (download.status !== "running") {
    speedSamples.set(id, { at: now, done, speed: 0 });
    return 0;
  }
  if (!previous) {
    speedSamples.set(id, { at: now, done, speed: 0 });
    return 0;
  }

  const seconds = (now - previous.at) / 1000;
  if (seconds < 0.25) return previous.speed;

  const bytes = done - previous.done;
  let speed = bytes > 0 ? bytes / seconds : 0;
  if (previous.speed > 0 && speed > 0) {
    speed = previous.speed * 0.6 + speed * 0.4;
  }
  speedSamples.set(id, { at: now, done, speed });
  return speed;
}

function renderDownloads(downloads, { offline }) {
  const active = downloads
    .filter(
      (download) =>
        download &&
        (download.status === "running" ||
          download.status === "queued" ||
          download.status === "paused"),
    )
    .sort((a, b) => b.id - a.id)
    .slice(0, MAX_ROWS);

  const ids = new Set(active.map((download) => String(download.id)));
  for (const [id, row] of rows) {
    if (ids.has(id)) continue;
    clearTimeout(row.cancelTimer);
    row.el.remove();
    rows.delete(id);
    speedSamples.delete(Number(id));
  }

  for (const download of active) {
    const id = String(download.id);
    let row = rows.get(id);
    if (!row) {
      row = createRow(download);
      rows.set(id, row);
      refs.downloads.insertBefore(row.el, refs.downloads.firstElementChild);
    }
    updateRow(row, download);
  }

  refs.count.textContent = active.length
    ? `${active.length} active`
    : "";
  refs.empty.hidden = active.length > 0;
  refs.empty.textContent = offline
    ? "Start B Download Manager to connect."
    : "No active downloads.";
}

/* --------------------------------------------------------------- actions */

async function showInApp(row) {
  if (row.busy) return;
  row.busy = true;
  const answer = await message({ type: "bdm:control", id: row.id, action: "show" });
  row.busy = false;
  if (answer?.ok) {
    window.close();
    return;
  }
  setConnection("warn", actionText(answer?.error));
  schedule(OFFLINE_POLL_MS);
}

async function togglePause(row) {
  if (row.busy) return;
  row.busy = true;
  row.pause.disabled = true;
  const paused = row.el.dataset.status === "paused";
  const answer = await message({
    type: "bdm:control",
    id: row.id,
    action: paused ? "resume" : "pause",
  });
  row.busy = false;
  row.pause.disabled = false;

  if (!answer?.ok) {
    setConnection("warn", actionText(answer?.error));
    schedule(OFFLINE_POLL_MS);
    return;
  }
  if (answer.status) updateRow(row, answer.status);
  refreshSoon();
}

function onCancel(row) {
  if (row.busy) return;

  if (row.cancel.dataset.confirming !== "true") {
    clearTimeout(row.cancelTimer);
    row.cancel.dataset.confirming = "true";
    row.cancel.title = "Click again to stop";
    row.cancel.setAttribute("aria-label", "Confirm stop download");
    row.cancelTimer = setTimeout(() => resetCancel(row), 3000);
    return;
  }

  resetCancel(row);
  void stopDownload(row);
}

function resetCancel(row) {
  clearTimeout(row.cancelTimer);
  row.cancelTimer = 0;
  row.cancel.dataset.confirming = "false";
  row.cancel.title = "Stop download";
  row.cancel.setAttribute("aria-label", "Stop download");
}

async function stopDownload(row) {
  row.busy = true;
  row.cancel.disabled = true;
  const answer = await message({ type: "bdm:control", id: row.id, action: "cancel" });
  row.busy = false;
  row.cancel.disabled = false;

  if (!answer?.ok) {
    setConnection("warn", actionText(answer?.error));
    schedule(OFFLINE_POLL_MS);
    return;
  }
  if (answer.status) updateRow(row, answer.status);
  refreshSoon();
}

/* ----------------------------------------------------------------- media */

async function loadPageMedia() {
  let tabs;
  try {
    tabs = await browser.tabs.query({ active: true, currentWindow: true });
  } catch {
    return;
  }
  const tab = tabs?.[0];
  if (!tab || tab.id === undefined) return;

  let answer;
  try {
    answer = await browser.tabs.sendMessage(tab.id, { type: "bdm:media-list" });
  } catch {
    // A page without the content script, such as about:addons, has nothing.
    return;
  }
  if (closed || !answer?.ok) return;
  renderMedia(Array.isArray(answer.items) ? answer.items : [], answer.referer || "");
}

function renderMedia(items, referer) {
  const list = items.slice(0, MAX_MEDIA);
  refs.mediaList.textContent = "";
  refs.mediaSection.hidden = list.length === 0;
  refs.mediaCount.textContent = list.length ? String(list.length) : "";

  for (const item of list) {
    const li = document.createElement("li");
    li.className = "media-row";

    const tag = document.createElement("span");
    tag.className = "tag";
    tag.textContent = item.label || "FILE";

    const name = document.createElement("span");
    name.className = "media-name";
    name.textContent = item.name || bdmNameFromUrl(item.url) || "Media";
    name.title = item.url;

    const button = iconButton("download", `Download ${name.textContent}`);
    button.addEventListener("click", async () => {
      button.disabled = true;
      const answer = await message({
        type: "bdm:send",
        payload: {
          url: item.url,
          name: item.name || bdmNameFromUrl(item.url),
          referer,
        },
      });

      if (!answer?.ok) {
        button.disabled = false;
        setConnection("warn", actionText(answer?.error));
        schedule(OFFLINE_POLL_MS);
        return;
      }

      li.dataset.sent = "true";
      setIcon(button, "check");
      button.setAttribute("aria-label", "Sent to B Download Manager");
      refreshSoon();
    });

    li.append(tag, name, button);
    refs.mediaList.append(li);
  }
}

/* ---------------------------------------------------------------- polling */

async function refresh() {
  if (closed || inFlight) return;
  inFlight = true;
  clearTimeout(pollTimer);

  const answer = await message({ type: "bdm:list" });
  inFlight = false;
  if (closed) return;

  if (!answer || !answer.ok) {
    setConnection("offline", connectionText(answer?.error));
    renderDownloads([], { offline: true });
    schedule(OFFLINE_POLL_MS);
    return;
  }

  setConnection("ok", "Connected");
  renderDownloads(Array.isArray(answer.downloads) ? answer.downloads : [], {
    offline: false,
  });
  schedule(POLL_MS);
}

/* ------------------------------------------------------------------ start */

refs.retry.addEventListener("click", () => {
  setConnection("checking", "Connecting");
  void refresh();
});

refs.openApp.addEventListener("click", async () => {
  const answer = await message({ type: "bdm:focus" });
  if (!answer?.ok) {
    setConnection("warn", actionText(answer?.error));
    schedule(OFFLINE_POLL_MS);
  }
});

refs.settings.addEventListener("click", () => {
  void browser.runtime.openOptionsPage();
  window.close();
});

document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    clearTimeout(pollTimer);
  } else {
    void refresh();
  }
});

window.addEventListener("unload", () => {
  closed = true;
  clearTimeout(pollTimer);
});

bdmLoadSettings()
  .then((settings) => bdmApplyPageTheme(settings.theme))
  .catch(() => {});

browser.storage.onChanged.addListener((_changes, area) => {
  if (area !== "local") return;
  bdmLoadSettings()
    .then((settings) => bdmApplyPageTheme(settings.theme))
    .catch(() => {});
});

setConnection("checking", "Connecting");
void refresh();
/* The page list does not need the app, so it is read straight away. */
void loadPageMedia();
