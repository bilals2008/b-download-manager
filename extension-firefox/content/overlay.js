/*
  The floating download button IDM puts on a video.

  One button follows the video the pointer is over; clicking it opens a small
  panel with the files that video offers. The whole thing lives in a closed
  shadow root with a constructed stylesheet, so the page's CSS and its CSP can
  neither change it nor block it, and the page cannot reach into it.

  The overlay offers every address the application can fetch: the file an
  element names, or for a player that streams through MSE the requests that
  player made. A protected (EME) video and an HLS or DASH manifest are not
  offered, because the engine cannot download them as a file.
*/

(() => {
  if (window.__bdmOverlayInstalled) return;
  window.__bdmOverlayInstalled = true;

  const FAB_SIZE = 30;
  const FAB_GAP = 8;
  const PANEL_WIDTH = 260;
  const EDGE = 8;
  const MIN_WIDTH = 160;
  const MIN_HEIGHT = 90;
  const HIDE_DELAY = 420;
  const COUNT_DELAY = 500;

  /** The panel and the button, built as nodes rather than markup. */
  function buildUi(root) {
    const fab = document.createElement("button");
    fab.className = "fab";
    fab.type = "button";
    fab.setAttribute("aria-expanded", "false");
    fab.setAttribute("aria-label", "Download video");
    fab.append(bdmIcon("download"));

    const panel = document.createElement("section");
    panel.className = "panel";
    panel.setAttribute("role", "dialog");
    panel.setAttribute("aria-label", "Download a video");

    const head = document.createElement("header");
    head.className = "head";

    const headText = document.createElement("span");
    headText.className = "head-text";
    const title = document.createElement("span");
    title.className = "title";
    title.textContent = "Video";
    const caption = document.createElement("span");
    caption.className = "caption";
    caption.textContent = "B Download Manager";
    headText.append(title, caption);

    const close = document.createElement("button");
    close.className = "close";
    close.type = "button";
    close.setAttribute("aria-label", "Close");
    close.append(bdmIcon("close"));

    head.append(headText, close);

    const list = document.createElement("ul");
    list.className = "list";

    const foot = document.createElement("footer");
    foot.className = "foot";
    const status = document.createElement("span");
    status.className = "status";
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    const openApp = document.createElement("button");
    openApp.className = "text-button open-app";
    openApp.type = "button";
    openApp.textContent = "Open app";
    foot.append(status, openApp);

    const bar = document.createElement("div");
    bar.className = "bar";
    bar.hidden = true;
    const barFill = document.createElement("span");
    bar.append(barFill);

    panel.append(head, list, foot, bar);
    root.append(fab, panel);

    return { fab, panel, title, close, list, status, openApp, bar, barFill };
  }

  const state = {
    settings: bdmNormalizeSettings({}),
    ready: false,
    host: null,
    root: null,
    ui: null,
    video: null,
    hovered: null,
    sources: [],
    visible: false,
    open: false,
    fab: null,
    hideTimer: 0,
    positionFrame: 0,
    pollTimer: 0,
    pollId: null,
    lastSample: null,
    countTimer: 0,
    lastCount: -1,
    positionAttached: false,
    observer: null,
    sending: false,
  };

  function clamp(value, min, max) {
    return Math.min(Math.max(value, min), max);
  }

  /** The switch is on and this page is not one the user excluded. */
  function overlayAllowed() {
    return bdmAllowsOverlay(state.settings, location.hostname);
  }

  function videoInPath(path) {
    for (const node of path) {
      if (node instanceof HTMLVideoElement) return node;
    }
    return null;
  }

  function cancelHide() {
    if (state.hideTimer) {
      clearTimeout(state.hideTimer);
      state.hideTimer = 0;
    }
  }

  function scheduleHide() {
    cancelHide();
    state.hideTimer = setTimeout(() => {
      state.hideTimer = 0;
      if (!state.open) hide();
    }, HIDE_DELAY);
  }

  /* ------------------------------------------------------------- the host */

  function ensureHost() {
    if (state.host && state.host.isConnected) return;

    if (state.host) {
      // A page that replaced its root took the host with it.
      removeHost();
    }

    const host = document.createElement("div");
    host.setAttribute("data-bdm-overlay", "");
    host.dataset.theme = state.settings.theme;
    host.style.cssText =
      "all: initial; display: block; position: fixed; left: 0; top: 0; " +
      "width: 0; height: 0; overflow: visible; z-index: 2147483647;";

    const root = host.attachShadow({ mode: "closed" });
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(BDM_OVERLAY_STYLE);
    root.adoptedStyleSheets = [sheet];
    (document.documentElement || document.body).appendChild(host);

    const ui = buildUi(root);

    ui.fab.addEventListener("click", () => {
      if (state.open) closePanel(true);
      else openPanel();
    });
    ui.close.addEventListener("click", () => closePanel(true));
    ui.openApp.addEventListener("click", focusApp);
    ui.list.addEventListener("click", (event) => {
      const button = event.target.closest?.(".row-action");
      if (!button || button.disabled) return;
      const row = button.closest(".row");
      const url = row?.dataset.url;
      if (url) void send(url, row, button);
    });

    state.host = host;
    state.root = root;
    state.ui = ui;
    if (state.visible) host.dataset.visible = "true";
    if (state.open) host.dataset.open = "true";
  }

  function removeHost() {
    state.host?.remove();
    state.host = null;
    state.root = null;
    state.ui = null;
  }

  /* -------------------------------------------------------------- position */

  function attachPositionListeners() {
    if (state.positionAttached) return;
    state.positionAttached = true;
    document.addEventListener("scroll", schedulePosition, true);
    window.addEventListener("resize", schedulePosition);
  }

  function detachPositionListeners() {
    if (!state.positionAttached) return;
    state.positionAttached = false;
    document.removeEventListener("scroll", schedulePosition, true);
    window.removeEventListener("resize", schedulePosition);
  }

  function schedulePosition() {
    if (state.positionFrame) return;
    state.positionFrame = requestAnimationFrame(() => {
      state.positionFrame = 0;
      position();
    });
  }

  function position() {
    if (!state.visible || !state.video || !state.host) return;

    const rect = state.video.getBoundingClientRect();
    const onScreen =
      rect.width >= 40 &&
      rect.height >= 40 &&
      rect.bottom > EDGE &&
      rect.top < window.innerHeight - EDGE &&
      rect.right > EDGE &&
      rect.left < window.innerWidth - EDGE;
    if (!onScreen) {
      hide();
      return;
    }

    const x = clamp(
      rect.right - EDGE - FAB_SIZE,
      EDGE,
      window.innerWidth - EDGE - FAB_SIZE,
    );
    const y = clamp(
      rect.top + 10,
      EDGE,
      window.innerHeight - EDGE - FAB_SIZE,
    );

    state.fab = { x, y };
    state.host.style.transform = `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`;
    if (state.open) positionPanel();
  }

  function positionPanel() {
    if (!state.open || !state.fab || !state.ui) return;
    const panel = state.ui.panel;
    const height = panel.offsetHeight || 160;

    let left = state.fab.x + FAB_SIZE - PANEL_WIDTH;
    left = clamp(left, EDGE, Math.max(EDGE, window.innerWidth - EDGE - PANEL_WIDTH));

    let top = state.fab.y + FAB_SIZE + FAB_GAP;
    if (top + height > window.innerHeight - EDGE) {
      top = state.fab.y - FAB_GAP - height;
    }
    top = clamp(top, EDGE, Math.max(EDGE, window.innerHeight - EDGE - height));

    panel.style.left = `${Math.round(left - state.fab.x)}px`;
    panel.style.top = `${Math.round(top - state.fab.y)}px`;
  }

  /* --------------------------------------------------------------- showing */

  /**
   * What this video offers. A player that streams through MSE names no file of
   * its own, so the addresses the page requested stand in for it.
   */
  function sourcesFor(video) {
    if (video.mediaKeys) return [];
    const direct = bdmSourcesFor(video);
    return direct.length ? direct : bdmCapturedSources();
  }

  function showFor(video) {
    if (!state.ready || !overlayAllowed()) return;
    if (!(video instanceof HTMLVideoElement)) return;
    if (document.fullscreenElement) return;
    if (video.mediaKeys) return;

    const rect = video.getBoundingClientRect();
    if (rect.width < MIN_WIDTH || rect.height < MIN_HEIGHT) return;

    const sources = sourcesFor(video);
    if (!sources.length) {
      if (state.video === video) hide();
      return;
    }

    const changed = state.video !== video;
    state.video = video;
    state.sources = sources;
    if (changed) {
      state.lastSample = null;
      state.pollId = null;
    }

    ensureHost();
    updateHeading();
    state.host.dataset.theme = state.settings.theme;

    if (!state.visible) {
      state.visible = true;
      state.host.dataset.visible = "true";
      attachPositionListeners();
    }

    if (state.open && changed) renderSources();
    position();
  }

  function updateHeading() {
    if (!state.ui) return;
    const first = state.sources[0];
    const name = bdmIsCapturedMedia(first) ? bdmPageName() : bdmNameFromUrl(first) || bdmPageName();
    state.ui.title.textContent = name;
    state.ui.fab.setAttribute("aria-label", `Download ${name} with B Download Manager`);
    state.ui.fab.title = `Download ${name}`;
  }

  function hide() {
    cancelHide();
    closePanel(false);
    detachPositionListeners();
    if (state.host) {
      state.host.dataset.visible = "false";
      state.host.dataset.open = "false";
    }
    state.visible = false;
    state.video = null;
    state.hovered = null;
    state.sources = [];
    state.fab = null;
    if (state.positionFrame) {
      cancelAnimationFrame(state.positionFrame);
      state.positionFrame = 0;
    }
  }

  /* --------------------------------------------------------------- panel */

  function openPanel() {
    if (!state.visible || !state.sources.length) return;
    ensureHost();
    state.open = true;
    state.host.dataset.open = "true";
    state.ui.fab.dataset.expanded = "true";
    state.ui.fab.setAttribute("aria-expanded", "true");
    setStatus("", "");
    setProgress(0, true);
    renderSources();
    position();
    const target = state.ui.list.querySelector(".row-action") || state.ui.close;
    target.focus({ preventScroll: true });
  }

  function closePanel(focusFab) {
    const wasOpen = state.open;
    state.open = false;
    stopPolling();
    if (state.host) state.host.dataset.open = "false";
    if (state.ui) {
      state.ui.fab.dataset.expanded = "false";
      state.ui.fab.setAttribute("aria-expanded", "false");
      setStatus("", "");
      setProgress(0, true);
    }
    if (wasOpen && focusFab && state.ui) {
      state.ui.fab.focus({ preventScroll: true });
    }
  }

  function renderSources() {
    if (!state.ui) return;
    const list = state.ui.list;
    list.textContent = "";

    for (const url of state.sources) {
      const row = document.createElement("li");
      row.className = "row";
      row.dataset.url = url;

      const tag = document.createElement("span");
      tag.className = "tag";
      tag.textContent = bdmMediaLabel(url);

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = bdmMediaName(url, "");
      name.title = url;

      const button = document.createElement("button");
      button.type = "button";
      button.className = "row-action";
      button.append(bdmIcon("download"));
      button.setAttribute("aria-label", `Download ${name.textContent} with B Download Manager`);

      row.append(tag, name, button);
      list.append(row);
    }

    if (!state.sources.length) {
      const empty = document.createElement("li");
      empty.className = "empty";
      empty.textContent = "Nothing here can be downloaded.";
      list.append(empty);
    }
  }

  function setStatus(text, tone) {
    if (!state.ui) return;
    state.ui.status.textContent = text || "";
    state.ui.status.dataset.tone = tone || "";
    state.ui.status.title = text || "";
  }

  function setProgress(percent, hidden) {
    if (!state.ui) return;
    const value = Math.max(0, Math.min(100, Number(percent) || 0));
    state.ui.bar.hidden = Boolean(hidden) || value <= 0;
    state.ui.barFill.style.width = `${value}%`;
  }

  function statusForError(error) {
    if (!error) return "The app could not take it";
    switch (error.kind) {
      case "offline":
        return "B Download Manager is not running";
      case "timeout":
        return "The app is not responding";
      case "protocol":
        return "The bridge versions do not match";
      case "refused":
      case "http":
        return error.message || "The app refused the download";
      default:
        return "The app could not take it";
    }
  }

  /* ---------------------------------------------------------------- send */

  async function send(url, row, button) {
    if (state.sending) return;
    state.sending = true;
    button.disabled = true;
    setProgress(0, true);
    setStatus("Sending to B Download Manager…", "");

    try {
      const answer = await browser.runtime.sendMessage({
        type: "bdm:send",
        // A video is one of several streams; the dialog names it and picks the
        // folder before anything starts.
        mode: "ask",
        payload: {
          url,
          name: bdmMediaFileName(url, bdmPageName()),
          referer: location.href,
        },
      });

      if (!answer || !answer.ok) {
        setStatus(statusForError(answer?.error), "error");
        button.disabled = false;
        return;
      }

      row.dataset.sent = "true";
      button.replaceChildren(bdmIcon("check"));
      button.setAttribute("aria-label", "Sent to B Download Manager");

      const result = answer.result;
      if (result?.mode === "direct" && Number.isInteger(result.id)) {
        setStatus("Downloading…", "ok");
        startPolling(result.id);
      } else {
        setStatus("Confirm it in the app", "ok");
      }
    } catch {
      setStatus("B Download Manager is not running", "error");
      button.disabled = false;
    } finally {
      state.sending = false;
    }
  }

  function startPolling(id) {
    stopPolling();
    state.pollId = id;
    state.lastSample = null;
    state.pollTimer = setTimeout(pollDownload, 900);
  }

  function stopPolling() {
    if (state.pollTimer) {
      clearTimeout(state.pollTimer);
      state.pollTimer = 0;
    }
    state.pollId = null;
    state.lastSample = null;
  }

  async function pollDownload() {
    state.pollTimer = 0;
    const id = state.pollId;
    if (!state.open || id === null) return;

    let answer;
    try {
      answer = await browser.runtime.sendMessage({ type: "bdm:list" });
    } catch {
      answer = null;
    }
    if (!state.open || state.pollId !== id) return;

    if (!answer || !answer.ok) {
      setStatus(statusForError(answer?.error), "error");
      return;
    }

    const download = (answer.downloads || []).find((item) => item.id === id);
    if (!download) {
      setStatus("Started in the app", "ok");
      return;
    }

    if (download.status === "completed") {
      setStatus("Saved by B Download Manager", "ok");
      setProgress(100, false);
      stopPolling();
      return;
    }
    if (download.status === "failed" || download.status === "cancelled") {
      setStatus(download.error || "The app stopped this download", "error");
      stopPolling();
      return;
    }
    if (download.status === "paused") {
      setStatus("Paused in the app", "");
      setProgress(bdmPercentOf(download), false);
    } else {
      const percent = bdmPercentOf(download);
      const speed = sampleSpeed(download);
      setProgress(percent, false);
      setStatus(
        speed
          ? `Downloading ${percent.toFixed(0)}% · ${speed}`
          : `Downloading ${percent.toFixed(0)}%`,
        "ok",
      );
    }

    state.pollTimer = setTimeout(pollDownload, 1000);
  }

  function sampleSpeed(download) {
    const now = performance.now();
    const previous = state.lastSample;
    state.lastSample = { at: now, downloaded: download.downloaded };
    if (!previous) return "";
    const seconds = (now - previous.at) / 1000;
    const bytes = download.downloaded - previous.downloaded;
    if (seconds <= 0.15 || seconds > 12 || bytes <= 0) return "";
    return bdmFormatSpeed(bytes / seconds);
  }

  async function focusApp() {
    let answer;
    try {
      answer = await browser.runtime.sendMessage({ type: "bdm:focus" });
    } catch {
      answer = null;
    }
    if (answer?.ok) setStatus("Brought the app forward", "ok");
    else setStatus("B Download Manager is not running", "error");
  }

  /* --------------------------------------------------------------- count */

  function sendCount(count) {
    state.lastCount = count;
    browser.runtime.sendMessage({ type: "bdm:media-count", count }).catch(() => {});
  }

  function scheduleCountRefresh() {
    if (!state.ready || !overlayAllowed()) return;
    if (state.countTimer) clearTimeout(state.countTimer);
    state.countTimer = setTimeout(() => {
      state.countTimer = 0;
      refreshCount();
    }, COUNT_DELAY);
  }

  function refreshCount() {
    if (!state.ready) return;
    const count = overlayAllowed() ? bdmMediaCount() : 0;
    if (count !== state.lastCount) sendCount(count);
  }

  function startObserver() {
    if (state.observer || !document.documentElement) return;
    bdmWatchMediaRequests(() => {
      scheduleCountRefresh();
      // The streams often arrive after the pointer is already on the video.
      if (state.hovered?.isConnected) showFor(state.hovered);
    });
    state.observer = new MutationObserver((records) => {
      const touchesMedia = records.some((record) => {
        if (record.target instanceof HTMLMediaElement) return true;
        for (const node of [...record.addedNodes, ...record.removedNodes]) {
          if (node.nodeType !== 1) continue;
          if (node.matches?.("video, audio")) return true;
          if (node.querySelector?.("video, audio")) return true;
        }
        return false;
      });
      if (!touchesMedia) return;
      scheduleCountRefresh();
      if (state.visible && state.video && !state.video.isConnected) hide();
    });
    state.observer.observe(document.documentElement, { childList: true, subtree: true });
  }

  function stopObserver() {
    state.observer?.disconnect();
    state.observer = null;
  }

  /* ------------------------------------------------------------- events */

  function onPointerOver(event) {
    if (!state.ready || !overlayAllowed()) return;
    const path = event.composedPath();
    if (state.host && path.includes(state.host)) {
      cancelHide();
      return;
    }

    const video = videoInPath(path);
    state.hovered = video;
    if (video) {
      cancelHide();
      showFor(video);
      return;
    }

    if (!state.open && state.visible) scheduleHide();
  }

  function onPointerDown(event) {
    if (!state.open) return;
    const path = event.composedPath();
    if (state.host && path.includes(state.host)) return;
    closePanel(false);
    scheduleHide();
  }

  function onKeyDown(event) {
    if (event.key !== "Escape") return;
    if (state.open) {
      event.preventDefault();
      event.stopPropagation();
      closePanel(true);
    } else if (state.visible) {
      hide();
    }
  }

  function onFocusIn(event) {
    if (!state.ready || !overlayAllowed()) return;
    const path = event.composedPath();
    if (state.host && path.includes(state.host)) return;
    const video = videoInPath(path);
    if (video) {
      showFor(video);
    } else if (state.visible && !state.open) {
      scheduleHide();
    }
  }

  function onMediaChange(event) {
    const video = event.target;
    if (!(video instanceof HTMLVideoElement)) return;

    scheduleCountRefresh();
    if (video !== state.video) return;

    state.sources = sourcesFor(video);
    if (!state.sources.length || video.mediaKeys) {
      hide();
      return;
    }

    updateHeading();
    if (state.open) {
      renderSources();
      position();
    }
  }

  function onSettingsChanged(changes, area) {
    if (area !== "local") return;
    void bdmLoadSettings().then((settings) => {
      const wasAllowed = bdmAllowsOverlay(state.settings, location.hostname);
      state.settings = settings;
      const allowed = bdmAllowsOverlay(settings, location.hostname);
      if (state.host) state.host.dataset.theme = settings.theme;

      if (!allowed) {
        hide();
        removeHost();
        stopObserver();
        if (wasAllowed || state.lastCount !== 0) sendCount(0);
        return;
      }

      if (!wasAllowed) {
        state.lastCount = -1;
        startObserver();
        refreshCount();
      }
    });
  }

  function applySettings() {
    if (!overlayAllowed()) return;
    state.host && (state.host.dataset.theme = state.settings.theme);
  }

  function destroy() {
    hide();
    stopObserver();
    removeHost();
    if (state.countTimer) clearTimeout(state.countTimer);
    if (state.positionFrame) cancelAnimationFrame(state.positionFrame);
    state.ready = false;
  }

  /* --------------------------------------------------------------- start */

  document.addEventListener("pointerover", onPointerOver, true);
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeyDown, true);
  document.addEventListener("focusin", onFocusIn, true);
  document.addEventListener("loadedmetadata", onMediaChange, true);
  document.addEventListener("durationchange", onMediaChange, true);
  document.addEventListener("emptied", onMediaChange, true);
  document.addEventListener("fullscreenchange", () => {
    if (document.fullscreenElement) hide();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) hide();
  });
  window.addEventListener("pagehide", (event) => {
    if (!event.persisted) destroy();
  });
  browser.storage.onChanged.addListener(onSettingsChanged);

  browser.runtime.onMessage.addListener((message) => {
    if (!message || message.type !== "bdm:media-list") return undefined;
    // The popup can ask on a page whose overlay is switched off.
    bdmWatchMediaRequests();
    return Promise.resolve({
      ok: true,
      items: bdmPageMedia(),
      referer: location.href,
    });
  });

  void bdmLoadSettings()
    .catch(() => bdmNormalizeSettings({}))
    .then((settings) => {
      state.settings = settings;
      state.ready = true;
      applySettings();
      if (overlayAllowed()) {
        startObserver();
        refreshCount();
      } else {
        sendCount(0);
      }
    });
})();
