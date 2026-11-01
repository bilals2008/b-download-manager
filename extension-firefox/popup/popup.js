/* The popup: connection state, the downloads in flight and their controls. */

const refs = {
  status: document.getElementById("status"),
  statusText: document.getElementById("status-text"),
  retry: document.getElementById("retry"),
  openApp: document.getElementById("open-app"),
  allLinks: document.getElementById("all-links"),
  settings: document.getElementById("settings"),
  downloads: document.getElementById("downloads"),
  count: document.getElementById("download-count"),
  empty: document.getElementById("downloads-empty"),
  mediaSection: document.getElementById("media-section"),
  mediaList: document.getElementById("media-list"),
  mediaCount: document.getElementById("media-count"),
  mediaTitle: document.getElementById("media-title"),
  mediaBack: document.getElementById("media-back"),
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

/** The inline quality list the popup draws for a media job. */
const mediaQuality = {
  items: [],
  referer: "",
  job: 0,
  jobs: [],
  view: null,
  timer: 0,
  done: false,
};

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

function messageRow(text) {
  const li = document.createElement("li");
  li.className = "media-row";
  const span = document.createElement("span");
  span.className = "media-name";
  span.textContent = text;
  li.append(span);
  return li;
}

/** The quality rows of a job, sound-only rows last. */
function qualityRows(view) {
  const formats = new Map(
    (view.formats || []).map((format) => [format.id, format]),
  );
  const videos = [];
  const audios = [];
  for (const pair of view.pairs || []) {
    const video = formats.get(pair.video);
    if (!video || video.kind === "audio") continue;
    const audio = pair.audio ? formats.get(pair.audio) : null;
    let size = video.size ?? null;
    if (size !== null && audio) {
      size = audio.size === null ? size : size + audio.size;
    }
    videos.push({
      id: video.id,
      audio: pair.audio || null,
      label: pair.label || video.label || video.id,
      size,
      note: video.kind === "video" && !pair.audio ? "No sound" : "",
      height: Number.isFinite(video.height) ? video.height : -1,
    });
  }
  const used = new Set(
    (view.pairs || []).map((pair) => pair.audio).filter(Boolean),
  );
  for (const format of view.formats || []) {
    if (format.kind === "audio" && !used.has(format.id)) {
      audios.push({
        id: format.id,
        audio: null,
        label: format.label || format.id,
        size: format.size ?? null,
        note: "",
        height: -1,
      });
    }
  }
  videos.sort(
    (left, right) =>
      right.height - left.height ||
      Number(Boolean(left.note)) - Number(Boolean(right.note)),
  );
  return [...videos, ...audios];
}

function renderQualities(view) {
  refs.mediaTitle.textContent = "Qualities";
  refs.mediaBack.hidden = false;
  refs.mediaCount.textContent = "";
  refs.mediaList.textContent = "";

  if (view.cookiesOffered && !view.cookieConsent) {
    const consent = messageRow("This site sent its cookies.");
    const allow = document.createElement("button");
    allow.type = "button";
    allow.className = "text-button";
    allow.textContent = "Allow cookies";
    allow.addEventListener("click", async () => {
      const answer = await message({
        type: "bdm:media-allow",
        job: mediaQuality.job,
      });
      if (answer?.ok && answer.view) {
        mediaQuality.view = answer.view;
        renderQualities(answer.view);
      }
    });
    consent.append(allow);
    refs.mediaList.append(consent);
  }

  if (view.state === "failed") {
    refs.mediaList.append(
      messageRow(view.error || "This video cannot be listed."),
    );
    return;
  }

  const rows = qualityRows(view);
  if (!rows.length) {
    refs.mediaList.append(messageRow("No qualities were found."));
    return;
  }

  for (const row of rows) {
    const li = document.createElement("li");
    li.className = "media-row";

    const info = document.createElement("span");
    info.className = "quality-info";
    const name = document.createElement("span");
    name.className = "media-name";
    name.textContent = row.label;
    info.append(name);
    if (row.note) {
      const note = document.createElement("span");
      note.className = "quality-note";
      note.textContent = row.note;
      info.append(note);
    }

    const size = document.createElement("span");
    size.className = "quality-size";
    size.textContent = row.size ? bdmFormatSize(row.size) : "";

    const button = iconButton("download", `Download ${row.label}`);
    button.addEventListener("click", async () => {
      button.disabled = true;
      const answer = await message({
        type: "bdm:media-start",
        job: mediaQuality.view?.job || mediaQuality.job,
        payload: {
          video: row.id,
          audio: row.audio,
          name: view.title || "",
        },
      });
      if (!answer?.ok) {
        button.disabled = false;
        refs.mediaCount.textContent = actionText(answer?.error);
        return;
      }
      mediaQuality.done = true;
      clearTimeout(mediaQuality.timer);
      mediaQuality.timer = 0;
      refs.mediaTitle.textContent = "Downloading in B Download Manager";
      refs.mediaList.textContent = "";
      setTimeout(() => window.close(), 800);
    });

    li.append(info, size, button);
    refs.mediaList.append(li);
  }
}

async function pollQuality() {
  const ids =
    mediaQuality.jobs.length > 0
      ? [...mediaQuality.jobs]
      : mediaQuality.job
        ? [mediaQuality.job]
        : [];
  if (!ids.length || closed) return;
  const score = (view) =>
    (view.pairs?.length || 0) * 10 + (view.formats?.length || 0);
  const answers = await Promise.all(
    ids.map((job) => message({ type: "bdm:media-status", job })),
  );
  if (closed) return;
  const views = answers
    .map((answer) => (answer?.ok ? answer.view : null))
    .filter(Boolean);
  const ready = views.filter((view) => view.state === "ready");
  const best = (ready.length ? ready : views)
    .slice()
    .sort((left, right) => score(right) - score(left))[0];
  if (best) {
    mediaQuality.job = best.job;
    mediaQuality.view = best;
    renderQualities(best);
  } else {
    const refused = answers.find((answer) => answer?.error);
    if (refused) {
      refs.mediaList.textContent = "";
      refs.mediaList.append(messageRow(actionText(refused.error)));
      return;
    }
  }
  mediaQuality.timer = setTimeout(() => void pollQuality(), 800);
}

async function beginMediaQuality(item, items, referer) {
  const manifestTargets = [...new Set(
    items
      .filter((entry) => bdmIsManifestUrl(entry.url))
      .map((entry) => entry.url),
  )].slice(0, 4);
  const targets = bdmIsManifestUrl(item.url)
    ? manifestTargets.length
      ? manifestTargets
      : [item.url]
    : bdmItagOf(item.url)
      ? [
          ...new Set(
            items
              .filter((entry) => bdmItagOf(entry.url))
              .map((entry) => entry.url),
          ),
        ]
      : [item.url];
  if (!targets.includes(item.url)) targets.unshift(item.url);

  clearTimeout(mediaQuality.timer);
  mediaQuality.timer = 0;
  mediaQuality.job = 0;
  mediaQuality.jobs = [];
  mediaQuality.view = null;
  mediaQuality.done = false;
  refs.mediaTitle.textContent = "Qualities";
  refs.mediaBack.hidden = false;
  refs.mediaCount.textContent = "";
  refs.mediaList.textContent = "";
  refs.mediaList.append(messageRow("Reading the qualities…"));

  const multipleManifests = targets.length > 1 && targets.every((target) => bdmIsManifestUrl(target));
  const jobs = [];
  for (const target of targets) {
    const answer = await message({
      type: "bdm:media",
      payload: {
        manifestUrl: target,
        pageUrl: referer,
        title: item.name || "",
        referer,
        kind: bdmIsManifestUrl(target) ? bdmManifestKind(target) : "file",
        presentation: "inline",
      },
    });
    if (!answer?.ok) {
      refs.mediaList.textContent = "";
      refs.mediaList.append(messageRow(actionText(answer?.error)));
      return;
    }
    const job = Number(answer.result?.job);
    if (Number.isInteger(job) && job > 0) jobs.push(job);
  }
  if (!jobs.length) {
    refs.mediaList.textContent = "";
    refs.mediaList.append(messageRow("The app did not name the media job."));
    return;
  }
  mediaQuality.jobs = multipleManifests ? jobs : [jobs[0]];
  mediaQuality.job = mediaQuality.jobs[0];
  void pollQuality();
}

function resetQuality() {
  clearTimeout(mediaQuality.timer);
  mediaQuality.timer = 0;
  mediaQuality.job = 0;
  mediaQuality.jobs = [];
  mediaQuality.view = null;
  mediaQuality.done = false;
  refs.mediaBack.hidden = true;
}

refs.mediaBack.addEventListener("click", () => {
  resetQuality();
  renderMedia(mediaQuality.items, mediaQuality.referer);
});

function renderMedia(items, referer) {
  if (mediaQuality.job && !mediaQuality.done) return;
  mediaQuality.items = items;
  mediaQuality.referer = referer;
  refs.mediaTitle.textContent = "On this page";
  refs.mediaBack.hidden = true;
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
    button.addEventListener("click", () => {
      button.disabled = true;
      void beginMediaQuality(item, items, referer);
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

refs.allLinks.addEventListener("click", async () => {
  let tabId;
  try {
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    tabId = tabs?.[0]?.id;
  } catch {
    return;
  }

  const answer = await message({ type: "bdm:links-open", tabId });
  if (answer?.ok) {
    window.close();
    return;
  }
  setConnection("warn", actionText(answer?.error));
  schedule(OFFLINE_POLL_MS);
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
