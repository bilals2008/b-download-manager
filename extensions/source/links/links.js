/*
  The window that turns a page full of links into a batch.

  The content script reads the links and classifies them; this page decides
  what is shown, what is ticked, and what is sent. Nothing is downloaded from
  here: the ticked links go to the application in chunks, and every chunk
  answers what happened to each link, so a page with some dead links still
  starts the ones that work.
*/

const refs = {
  state: document.getElementById("state"),
  stateText: document.getElementById("state-text"),
  retry: document.getElementById("retry"),
  pageName: document.getElementById("page-name"),
  close: document.getElementById("close"),
  category: document.getElementById("category"),
  extension: document.getElementById("extension"),
  pagesRow: document.getElementById("pages-row"),
  pages: document.getElementById("pages"),
  pagesCount: document.getElementById("pages-count"),
  selectShown: document.getElementById("select-shown"),
  clearAll: document.getElementById("clear-all"),
  links: document.getElementById("links"),
  empty: document.getElementById("empty"),
  report: document.getElementById("report"),
  reportTitle: document.getElementById("report-title"),
  reportList: document.getElementById("report-list"),
  folder: document.getElementById("folder"),
  queue: document.getElementById("queue"),
  connections: document.getElementById("connections"),
  selection: document.getElementById("selection"),
  send: document.getElementById("send"),
  cancel: document.getElementById("cancel"),
  note: document.getElementById("note"),
};

/** How many links one bridge request carries. The app takes up to 25. */
const SEND_CHUNK = 10;

const state = {
  tabId: null,
  scanId: "",
  scanned: false,
  page: { url: "", host: "", title: "" },
  links: [],
  stats: {},
  selected: new Set(),
  sent: new Set(),
  rows: new Map(),
  results: [],
  counts: { started: 0, queued: 0, skipped: 0, duplicates: 0, invalid: 0, refused: 0 },
  options: null,
  offline: false,
  busy: false,
  failures: 0,
};

/* ---------------------------------------------------------------- helpers */

function message(payload) {
  return browser.runtime.sendMessage(payload).catch(() => ({
    ok: false,
    error: { kind: "offline", message: "The extension is restarting." },
  }));
}

function plural(count, one, many) {
  return `${count} ${count === 1 ? one : many}`;
}

/** The status line, with the tone the dot takes. */
function setState(tone, text, options = {}) {
  refs.state.dataset.state = tone;
  refs.stateText.textContent = text;
  refs.stateText.title = options.title || text;
  refs.retry.hidden = !options.retry;
}

/** The host and the folder above a file name, for the second line of a row. */
function pathOf(url) {
  try {
    const parsed = new URL(url);
    const folder = parsed.pathname
      .split("/")
      .slice(0, -1)
      .filter(Boolean)
      .join("/");
    const base = parsed.protocol === "ftp:" ? `ftp://${parsed.hostname}` : parsed.hostname;
    return folder ? `${base}/${folder}` : base;
  } catch {
    return "";
  }
}

/** A fresh scan id, so an older scan cannot answer this window. */
function newScanId() {
  const random = globalThis.crypto?.randomUUID?.();
  return random || `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

/* ------------------------------------------------------------------ scan */

async function scan() {
  const params = new URLSearchParams(window.location.search);
  const tabId = Number(params.get("tab"));
  if (!Number.isInteger(tabId) || tabId <= 0) {
    state.scanned = true;
    setState("error", "This window was not given a page to read.");
    renderList();
    return;
  }

  state.tabId = tabId;
  state.scanId = newScanId();
  setState("checking", "Reading links…");

  const answer = await message({
    type: "bdm:links-scan",
    tabId,
    scanId: state.scanId,
  });

  state.scanned = true;
  if (!answer?.ok) {
    setState("error", answer?.error?.message || "The page could not be read.", {
      retry: true,
    });
    renderList();
    return;
  }

  applyScan(answer);
}

function applyScan(answer) {
  state.page = answer.page || { url: "", host: "", title: "" };
  state.stats = answer.stats || {};
  state.links = Array.isArray(answer.links) ? answer.links : [];
  state.selected = new Set();
  state.sent = new Set();
  state.rows = new Map();
  state.results = [];
  state.counts = { started: 0, queued: 0, skipped: 0, duplicates: 0, invalid: 0, refused: 0 };
  state.failures = 0;

  refs.pageName.textContent =
    state.page.title || state.page.host || "This page";
  refs.pageName.title = state.page.url;
  if (state.page.host) {
    document.title = `Download all links — ${state.page.host}`;
  }

  // The files are ticked; a link that is a page is left for the user to ask
  // for, because a page full of navigation is not a download.
  for (const item of state.links) {
    if (item.kind === "file") state.selected.add(item.key);
  }

  populateFilters();
  renderList();
  renderScanReport();
  updateSendState();

  if (state.stats.cancelled) {
    setState("warn", "Another link window took over this page.", { retry: true });
  } else if (!state.links.length) {
    setState(
      "warn",
      state.stats.truncated
        ? "This page has more links than one window can read."
        : "No links on this page can be downloaded.",
    );
  } else {
    setState(state.offline ? "warn" : "ok", describeScan());
  }

  // The options may already be in hand; otherwise ask now that the list is
  // here, so the send button becomes usable.
  if (state.options === null) void loadOptions();
}

/** "24 files · 3 page links · 4 left out". */
function describeScan() {
  const files = state.links.filter((item) => item.kind === "file").length;
  const pages = state.links.filter((item) => item.kind === "page").length;
  const skipped = Object.values(state.stats.skipped || {}).reduce(
    (total, count) => total + (Number(count) || 0),
    0,
  );
  const left = skipped + (Number(state.stats.duplicates) || 0);

  const parts = [plural(files, "file", "files")];
  if (pages) parts.push(plural(pages, "page link", "page links"));
  if (left) parts.push(`${left} left out`);
  return parts.join(" · ");
}

/* --------------------------------------------------------------- filters */

function populateFilters() {
  const categories = new Map();
  const extensions = new Map();
  let pages = 0;

  for (const item of state.links) {
    if (item.kind === "page") {
      pages += 1;
      continue;
    }
    categories.set(item.category, (categories.get(item.category) || 0) + 1);
    if (item.extension) {
      extensions.set(item.extension, (extensions.get(item.extension) || 0) + 1);
    }
  }

  const category = refs.category.value;
  refs.category.textContent = "";
  refs.category.append(new Option("All categories", ""));
  for (const name of BDM_LINK_CATEGORIES) {
    const count = categories.get(name) || 0;
    if (!count) continue;
    refs.category.append(new Option(`${name} (${count})`, name));
  }
  refs.category.value = [...refs.category.options].some(
    (option) => option.value === category,
  )
    ? category
    : "";

  const extension = refs.extension.value;
  refs.extension.textContent = "";
  refs.extension.append(new Option("All types", ""));
  for (const [name, count] of [...extensions.entries()].sort(
    (a, b) => b[1] - a[1] || a[0].localeCompare(b[0]),
  )) {
    refs.extension.append(new Option(`${name} (${count})`, name));
  }
  refs.extension.value = [...refs.extension.options].some(
    (option) => option.value === extension,
  )
    ? extension
    : "";

  refs.pagesRow.hidden = pages === 0;
  refs.pagesCount.textContent = pages ? String(pages) : "";
}

/** The links the filters show, page links included only when asked for. */
function visibleLinks() {
  const category = refs.category.value;
  const extension = refs.extension.value;
  return state.links.filter((item) => {
    if (item.kind === "page" && !refs.pages.checked) return false;
    if (category && item.category !== category) return false;
    if (extension && item.extension !== extension) return false;
    return true;
  });
}

/* ------------------------------------------------------------------- list */

function renderList() {
  const links = visibleLinks();
  refs.links.textContent = "";
  state.rows = new Map();

  const fragment = document.createDocumentFragment();
  for (const item of links) {
    const row = createRow(item);
    state.rows.set(item.key, row);
    fragment.append(row.el);
  }
  refs.links.append(fragment);

  refs.empty.hidden = links.length > 0;
  if (links.length === 0) {
    if (!state.scanned) refs.empty.textContent = "Reading links…";
    else if (!state.links.length) refs.empty.textContent = "No links found on this page.";
    else refs.empty.textContent = "No link matches these filters.";
  }

  renderFooter();
}

function createRow(item) {
  const el = document.createElement("li");
  el.className = "link";
  el.dataset.key = item.key;
  el.dataset.kind = item.kind;

  const label = document.createElement("label");
  label.className = "link-main";
  label.title = item.url;

  const box = document.createElement("input");
  box.type = "checkbox";
  // The wrapping label carries the badge and the path too; the file name is
  // what the control is about, so it names the checkbox on its own.
  box.setAttribute("aria-label", item.name);
  box.checked = state.selected.has(item.key);
  box.disabled = state.sent.has(item.key);

  const badge = document.createElement("span");
  badge.className = "badge";
  badge.textContent = item.badge || "FILE";

  const text = document.createElement("span");
  text.className = "link-text";
  const name = document.createElement("span");
  name.className = "link-name";
  name.textContent = item.name;
  const path = document.createElement("span");
  path.className = "link-path";
  path.textContent = pathOf(item.url);
  text.append(name, path);

  const kind = document.createElement("span");
  kind.className = "link-type";
  kind.textContent = item.kind === "page" ? "page" : item.category;

  label.append(box, badge, text, kind);
  el.append(label);

  el.dataset.checked = box.checked ? "true" : "false";
  if (state.sent.has(item.key)) el.dataset.sent = "true";

  box.addEventListener("change", () => {
    if (box.checked) state.selected.add(item.key);
    else state.selected.delete(item.key);
    el.dataset.checked = box.checked ? "true" : "false";
    renderFooter();
    updateSendState();
  });

  return { el, box };
}

function renderFooter() {
  const chosen = selectedLinks();
  const sent = state.sent.size;
  refs.selection.textContent = sent
    ? `${plural(chosen.length, "link", "links")} selected · ${sent} sent`
    : `${plural(chosen.length, "link", "links")} selected`;
  refs.send.textContent = state.busy
    ? "Sending…"
    : chosen.length
      ? `Download ${plural(chosen.length, "link", "links")}`
      : "Download links";
  refs.cancel.disabled = state.busy;
}

/** The ticked links that have not been sent already. */
function selectedLinks() {
  return state.links.filter(
    (item) => state.selected.has(item.key) && !state.sent.has(item.key),
  );
}

/** Whether the button may start a batch right now. */
function updateSendState() {
  refs.send.disabled =
    state.busy || selectedLinks().length === 0 || state.options === null;
  refs.folder.disabled = state.options === null;
  renderFooter();
}

/* ----------------------------------------------------------------- report */

/** Draw a list of { name, why } rows under a title. An empty list hides it. */
function renderReport(entries, title) {
  refs.report.hidden = entries.length === 0;
  if (entries.length === 0) {
    refs.reportList.textContent = "";
    return;
  }

  refs.reportTitle.textContent = title || "Report";
  refs.reportList.textContent = "";
  for (const entry of entries) {
    const li = document.createElement("li");
    const name = document.createElement("span");
    name.className = "report-name";
    name.textContent = entry.name;
    name.title = entry.name;
    const why = document.createElement("span");
    why.className = "report-why";
    why.textContent = entry.why;
    li.append(name, why);
    refs.reportList.append(li);
  }
}

/** What the scan left out, before anything is sent. */
function scanEntries() {
  const entries = [];
  const skipped = state.stats.skipped || {};
  for (const reason of ["stream", "scheme", "self", "unusable"]) {
    const count = Number(skipped[reason]) || 0;
    if (count) {
      entries.push({
        name: plural(count, "link", "links"),
        why: bdmLinkSkipLabel(reason),
      });
    }
  }

  const duplicates = Number(state.stats.duplicates) || 0;
  if (duplicates) {
    entries.push({
      name: plural(duplicates, "duplicate", "duplicates"),
      why: "the same address was listed once",
    });
  }

  if (state.stats.truncated) {
    entries.push({
      name: "Not all links",
      why: "this page holds more links than one window reads; the first ones are shown",
    });
  }

  return entries;
}

function renderScanReport() {
  renderReport(scanEntries(), "What was left out");
}

/** What happened to the links that did not start. */
function renderSendReport() {
  const entries = [];
  for (const item of state.results) {
    if (item.status === "started" || item.status === "queued") continue;
    entries.push({
      name: item.name || item.url,
      why: item.message || "The app did not take it.",
    });
  }

  renderReport(
    entries.length ? entries : scanEntries(),
    entries.length ? "What did not start" : "What was left out",
  );
}

/* ---------------------------------------------------------------- options */

async function loadOptions() {
  const answer = await message({ type: "bdm:batch-options" });
  if (!answer?.ok) {
    state.options = null;
    state.offline = true;
    fillFolders([]);
    updateSendState();
    if (state.scanned) {
      setState("warn", "B Download Manager is not running. Review the links, then open it and retry.", {
        retry: true,
      });
    }
    return;
  }

  state.options = answer;
  state.offline = false;
  fillFolders(Array.isArray(answer.folders) ? answer.folders : []);
  fillQueues(Array.isArray(answer.queues) ? answer.queues : []);
  if (Number(answer.connections) > 0) {
    refs.connections.value = String(answer.connections);
  }
  updateSendState();
  if (state.scanned && state.links.length) {
    setState("ok", describeScan());
  }
}

/** The places the app offered. The first is what an empty choice means. */
function fillFolders(folders) {
  const previous = refs.folder.value;
  refs.folder.textContent = "";

  const first = folders[0];
  refs.folder.append(
    new Option(first ? `Default: ${first.path}` : "App default", ""),
  );
  for (const folder of folders.slice(1)) {
    const option = new Option(folder.label || folder.path, folder.path);
    option.title = folder.path;
    refs.folder.append(option);
  }

  if ([...refs.folder.options].some((option) => option.value === previous)) {
    refs.folder.value = previous;
  }
  syncFolderTitle();
}

function fillQueues(queues) {
  if (!queues.length) return;
  const previous = refs.queue.value;
  refs.queue.textContent = "";
  for (const queue of queues) {
    refs.queue.append(new Option(queue.label || queue.id, queue.id));
  }
  if ([...refs.queue.options].some((option) => option.value === previous)) {
    refs.queue.value = previous;
  }
}

function syncFolderTitle() {
  const chosen = refs.folder.selectedOptions?.[0];
  refs.folder.title = chosen?.title || chosen?.textContent || "";
}

/* ------------------------------------------------------------------- send */

async function send() {
  if (state.busy) return;

  const chosen = selectedLinks();
  if (!chosen.length) return;

  if (state.options === null) {
    await loadOptions();
    if (state.options === null) {
      setState("error", "B Download Manager is not running.", { retry: true });
      return;
    }
  }

  state.busy = true;
  state.failures = 0;
  updateSendState();
  setState("sending", `Sending 0 of ${chosen.length}…`);

  let sent = 0;
  for (let index = 0; index < chosen.length; index += SEND_CHUNK) {
    const chunk = chosen.slice(index, index + SEND_CHUNK);
    const answer = await message({
      type: "bdm:batch",
      payload: {
        items: chunk.map((item) => ({
          url: item.url,
          name: item.name,
          category: bdmLinkAppCategory(item.category),
        })),
        folder: refs.folder.value,
        connections: Number(refs.connections.value) || 0,
        queue: refs.queue.value,
        referer: state.page.url,
      },
    });

    if (!answer?.ok) {
      state.busy = false;
      state.failures += 1;
      updateSendState();
      renderSendReport();
      setState(
        "error",
        `${sent} of ${chosen.length} sent. ${answer?.error?.message || "The app stopped answering."}`,
        { retry: true },
      );
      return;
    }

    const result = answer.result || {};
    for (const item of chunk) markSent(item.key);
    state.results.push(...(Array.isArray(result.items) ? result.items : []));
    accumulate(result.counts);
    sent += chunk.length;
    setState("sending", `Sending ${sent} of ${chosen.length}…`);
    renderFooter();
  }

  state.busy = false;
  updateSendState();
  renderSendReport();
  finishSend();
}

/** A link that was answered stays answered, even when it did not start. */
function markSent(key) {
  state.sent.add(key);
  const row = state.rows.get(key);
  if (!row) return;
  row.box.checked = false;
  row.box.disabled = true;
  row.el.dataset.checked = "false";
  row.el.dataset.sent = "true";
  state.selected.delete(key);
}

function accumulate(counts) {
  if (!counts || typeof counts !== "object") return;
  for (const key of Object.keys(state.counts)) {
    state.counts[key] += Number(counts[key]) || 0;
  }
}

function finishSend() {
  const { started, queued, duplicates, invalid, refused } = state.counts;
  const bits = [];
  if (started) bits.push(`${started} started`);
  if (queued) bits.push(`${queued} queued`);
  if (duplicates) bits.push(`${duplicates} already in the list`);
  if (invalid) bits.push(`${invalid} not downloadable`);
  if (refused) bits.push(`${refused} refused`);
  if (!bits.length) bits.push("Nothing was started");

  const worked = started + queued > 0;
  const clean = duplicates + invalid + refused === 0;
  setState(worked ? (clean ? "ok" : "warn") : "warn", bits.join(" · "));
  refs.note.textContent = queued
    ? "Queued links start when you press Start queue in the app."
    : worked
      ? "The app downloads each link itself. A file behind a sign-in may fail there."
      : "The app downloads each link itself. A link behind a sign-in or one that expired may fail there.";
}

/* ---------------------------------------------------------------- actions */

function selectShown() {
  for (const item of visibleLinks()) {
    if (state.sent.has(item.key)) continue;
    state.selected.add(item.key);
  }
  for (const [key, row] of state.rows) {
    const checked = state.selected.has(key);
    row.box.checked = checked;
    row.el.dataset.checked = checked ? "true" : "false";
  }
  renderFooter();
  updateSendState();
}

function clearAll() {
  state.selected.clear();
  for (const row of state.rows.values()) {
    row.box.checked = false;
    row.el.dataset.checked = "false";
  }
  renderFooter();
  updateSendState();
}

refs.close.addEventListener("click", () => window.close());
refs.cancel.addEventListener("click", () => window.close());
refs.send.addEventListener("click", () => void send());
refs.selectShown.addEventListener("click", selectShown);
refs.clearAll.addEventListener("click", clearAll);
refs.category.addEventListener("change", renderList);
refs.extension.addEventListener("change", renderList);
refs.pages.addEventListener("change", renderList);
refs.folder.addEventListener("change", syncFolderTitle);

refs.connections.addEventListener("change", () => {
  const value = Number(refs.connections.value);
  refs.connections.value = String(
    Number.isFinite(value) ? Math.min(32, Math.max(1, Math.round(value))) : 8,
  );
});

refs.retry.addEventListener("click", () => {
  if (state.failures > 0) {
    state.failures = 0;
    void loadOptions();
    if (!state.scanned) void scan();
    return;
  }
  if (!state.scanned || !state.links.length) {
    state.scanned = false;
    void scan();
    return;
  }
  void loadOptions();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    window.close();
    return;
  }

  const tag = event.target?.tagName;
  const typing = tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA";
  if (!typing && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
    event.preventDefault();
    selectShown();
  }
});

browser.runtime.onMessage.addListener((incoming) => {
  if (!incoming || incoming.type !== "bdm:links-scan-progress") return undefined;
  if (state.scanned || incoming.scanId !== state.scanId) return undefined;

  const total = Number(incoming.total) || 0;
  const scanned = Number(incoming.scanned) || 0;
  const found = Number(incoming.found) || 0;
  setState(
    "checking",
    total
      ? `Reading links… ${scanned} of ${total} checked, ${found} found`
      : `Reading links… ${plural(found, "link", "links")} found`,
  );
  return undefined;
});

/* ------------------------------------------------------------------ start */

function applyTheme() {
  bdmLoadSettings()
    .then((settings) => bdmApplyPageTheme(settings.theme))
    .catch(() => {});
}

applyTheme();
browser.storage.onChanged.addListener((_changes, area) => {
  if (area === "local") applyTheme();
});

setState("checking", "Reading links…");
updateSendState();
void loadOptions();
void scan();
