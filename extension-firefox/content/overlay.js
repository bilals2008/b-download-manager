/*
  The floating download button IDM puts on a video.

  One button follows the video the pointer is over; clicking it opens a small
  panel with the files that video offers. The whole thing lives in a closed
  shadow root with a constructed stylesheet, so the page's CSS and its CSP can
  neither change it nor block it, and the page cannot reach into it.

  A page decides where its video is, when it is replaced and what sits above
  the picture, so the button follows three signals rather than one event:

  - content/video-registry.js lists the <video> elements and names the one
    under the pointer, including a player whose own controls take the event;
  - the pointer's box against the video's box keeps the button alive while the
    pointer is over those controls, and lets it go when the pointer leaves the
    player;
  - a resize of the video, a scroll, a window resize or a fullscreen change
    moves it, because a player changes its size for all four. In fullscreen the
    host is mounted inside the fullscreen element, which is the only part of the
    page the top layer draws.

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
  /** How far outside the picture the pointer may be and still keep the button. */
  const POINTER_SLACK = 2;
  /** How often the download behind the button is read, in milliseconds. */
  const POLL_RUNNING = 1000;
  const POLL_HIDDEN = 4000;
  const POLL_WAITING = 1500;
  /** How long a hand-over may take to appear in the app before it is dropped. */
  const WAIT_WINDOW = 120000;

  /** The panel and the button, built as nodes rather than markup. */
  function buildUi(root) {
    const fab = document.createElement("button");
    fab.className = "fab";
    fab.type = "button";
    fab.setAttribute("aria-expanded", "false");
    fab.setAttribute("aria-haspopup", "dialog");
    fab.setAttribute("aria-label", "Download video");

    // The icon stays a node of its own so the progress ring beside it can be
    // left alone when the glyph changes to a check.
    const glyph = document.createElement("span");
    glyph.className = "glyph";
    glyph.append(bdmIcon("download"));

    const ring = document.createElementNS(BDM_SVG_NS, "svg");
    ring.setAttribute("class", "ring");
    ring.setAttribute("viewBox", "0 0 36 36");
    ring.setAttribute("aria-hidden", "true");
    ring.setAttribute("focusable", "false");
    const ringTrack = document.createElementNS(BDM_SVG_NS, "circle");
    ringTrack.setAttribute("class", "ring-track");
    const ringArc = document.createElementNS(BDM_SVG_NS, "circle");
    ringArc.setAttribute("class", "ring-arc");
    for (const circle of [ringTrack, ringArc]) {
      circle.setAttribute("cx", "18");
      circle.setAttribute("cy", "18");
      circle.setAttribute("r", "16");
      ring.append(circle);
    }
    // `pathLength` turns the dash pattern into a percentage of the circle, so
    // the arc is drawn straight from the percent the application reports.
    ringArc.setAttribute("pathLength", "100");

    fab.append(glyph, ring);

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

    return { fab, glyph, ringArc, panel, title, close, list, status, openApp, bar, barFill };
  }

  const state = {
    settings: bdmNormalizeSettings({}),
    ready: false,
    host: null,
    root: null,
    ui: null,
    // The video the button belongs to, and the one the pointer is on: a player
    // that streams through MSE has no address until it has asked for one, so
    // the pointer's video is remembered before there is anything to show.
    video: null,
    hovered: null,
    sources: [],
    visible: false,
    open: false,
    fab: null,
    hideTimer: 0,
    positionFrame: 0,
    countTimer: 0,
    lastCount: -1,
    listening: false,
    observedVideo: null,
    sizeObserver: null,
    stopRegistry: null,
    sending: false,
    // What the button last showed, so a hover does not rewrite it.
    progressStamp: "",
  };

  /**
   * The download the last hand-over started.
   *
   * The button shows how far it got, so the panel does not have to stay open.
   * A hand-over that asks first is followed by its address instead of its id:
   * the application answers with an id only when it started the download
   * itself, and the add dialog keeps the address it was given.
   */
  const track = {
    url: "",
    id: null,
    download: null,
    speed: "",
    sample: null,
    timer: 0,
    failures: 0,
    misses: 0,
    deadline: 0,
    done: false,
  };

  /** Every listener the script owns, so one abort takes them all away. */
  const pageEvents = new AbortController();

  function clamp(value, min, max) {
    return Math.min(Math.max(value, min), max);
  }

  /** The switch is on and this page is not one the user excluded. */
  function overlayAllowed() {
    return bdmAllowsOverlay(state.settings, location.hostname);
  }

  function within(rect, x, y, slack) {
    if (!rect || rect.width <= 0 || rect.height <= 0) return false;
    return (
      x >= rect.left - slack &&
      x <= rect.right + slack &&
      y >= rect.top - slack &&
      y <= rect.bottom + slack
    );
  }

  function fabRect() {
    if (!state.fab) return null;
    return { left: state.fab.x, top: state.fab.y, width: FAB_SIZE, height: FAB_SIZE };
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

  /** Where the host belongs: the fullscreen element, or the document. */
  function mountPoint() {
    const full = document.fullscreenElement;
    if (full && full.isConnected !== false) return full;
    return document.documentElement || document.body;
  }

  function ensureHost() {
    const mount = mountPoint();

    if (state.host && state.host.isConnected) {
      // A player that went fullscreen draws in the top layer; the host has to
      // live inside that element to be drawn with it.
      if (mount && state.host.parentNode !== mount) mount.appendChild(state.host);
      return;
    }

    if (state.host) {
      // A page that replaced its root took the host with it.
      removeHost();
    }
    if (!mount) return;

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

    // In fullscreen the host sits inside the player, so its own events must
    // stop at the host: otherwise a click on the button would reach the
    // player underneath and pause the video.
    const ownEvents = [
      "pointerdown", "pointerup", "mousedown", "mouseup",
      "click", "dblclick", "contextmenu", "wheel", "keydown", "keyup",
    ];
    for (const type of ownEvents) {
      host.addEventListener(type, (event) => event.stopPropagation());
    }

    mount.appendChild(host);

    state.host = host;
    state.root = root;
    state.ui = ui;
    state.progressStamp = "";
    if (state.visible) host.dataset.visible = "true";
    if (state.open) host.dataset.open = "true";
  }

  function removeHost() {
    state.host?.remove();
    state.host = null;
    state.root = null;
    state.ui = null;
    state.progressStamp = "";
  }

  /* -------------------------------------------------------------- position */

  function attachListeners() {
    if (state.listening) return;
    state.listening = true;
    document.addEventListener("scroll", schedulePosition, {
      capture: true,
      passive: true,
      signal: pageEvents.signal,
    });
    window.addEventListener("resize", schedulePosition, { signal: pageEvents.signal });
    // Only used to keep the button alive; it costs nothing until a hide is due.
    document.addEventListener("pointermove", onPointerMove, {
      capture: true,
      passive: true,
      signal: pageEvents.signal,
    });
  }

  function detachListeners() {
    if (!state.listening) return;
    state.listening = false;
    document.removeEventListener("scroll", schedulePosition, true);
    window.removeEventListener("resize", schedulePosition);
    document.removeEventListener("pointermove", onPointerMove, true);
  }

  /** A player changes size without a window resize: watch the element too. */
  function observeVideoSize(video) {
    if (typeof ResizeObserver !== "function") return;
    if (!state.sizeObserver) {
      state.sizeObserver = new ResizeObserver(() => schedulePosition());
    }
    if (state.observedVideo === video) return;
    if (state.observedVideo) state.sizeObserver.unobserve(state.observedVideo);
    state.observedVideo = video;
    state.sizeObserver.observe(video);
  }

  function unobserveVideoSize() {
    if (state.observedVideo) state.sizeObserver?.unobserve(state.observedVideo);
    state.observedVideo = null;
  }

  function schedulePosition() {
    if (state.positionFrame) return;
    state.positionFrame = requestAnimationFrame(() => {
      state.positionFrame = 0;
      position();
    });
  }

  function position() {
    if (!state.visible || !state.video) return;
    // A player that rebuilt its controls may have thrown the host away.
    if (!state.host || !state.host.isConnected) ensureHost();
    if (!state.host) return;

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

  function sameSources(a, b) {
    if (a.length !== b.length) return false;
    return a.every((url, index) => url === b[index]);
  }

  /**
   * Show the button for a video, or remember it until it has an address.
   *
   * A stream that the player has not asked for yet has nothing to offer, and a
   * button that opens an empty panel is worse than no button. The element is
   * kept as the pointer's video instead, so the first request that arrives
   * brings the button up while the pointer is still on the player.
   */
  function present(video) {
    if (!state.ready || !overlayAllowed()) return;
    if (!bdmIsVideo(video) || !video.isConnected) return;
    if (video.mediaKeys) {
      forget(video);
      return;
    }

    // A player that navigated inside this document belongs to a new page now,
    // and the download of the previous one is not this video's.
    if (bdmCaptureSyncPage()) stopTracking();

    const rect = video.getBoundingClientRect();
    if (rect.width < MIN_WIDTH || rect.height < MIN_HEIGHT) return;

    state.hovered = video;

    const sources = sourcesFor(video);
    if (!sources.length) {
      // The pointer is still on the player, so the video stays the pointer's
      // video and the next stream brings the button back without a new hover.
      if (state.video === video) hide(true);
      return;
    }

    const changed = state.video !== video || !sameSources(state.sources, sources);
    state.video = video;
    state.sources = sources;

    ensureHost();
    if (!state.host) return;
    state.host.dataset.theme = state.settings.theme;
    updateHeading();

    if (!state.visible) {
      state.visible = true;
      state.host.dataset.visible = "true";
      attachListeners();
    }
    observeVideoSize(video);

    if (state.open && changed) renderSources();
    applyProgress();
    position();
  }

  /** A video is gone, protected, or too small: nothing may point at it. */
  function forget(video) {
    if (state.hovered === video) state.hovered = null;
    if (state.video === video) hide();
  }

  function updateHeading() {
    if (!state.ui) return;
    const first = state.sources[0];
    const name = bdmIsCapturedMedia(first) ? bdmPageName() : bdmNameFromUrl(first) || bdmPageName();
    state.ui.title.textContent = name;
    state.ui.fab.setAttribute("aria-label", `Download ${name} with B Download Manager`);
    state.ui.fab.title = `Download ${name}`;
  }

  /**
   * Take the button away.
   *
   * `keepHovered` is for a player that has nothing to offer for a moment: the
   * pointer has not moved, so the element stays the pointer's video and the
   * next stream brings the button back.
   */
  function hide(keepHovered) {
    cancelHide();
    closePanel(false);
    detachListeners();
    unobserveVideoSize();
    if (state.host) {
      state.host.dataset.visible = "false";
      state.host.dataset.open = "false";
    }
    state.visible = false;
    state.video = null;
    if (!keepHovered) state.hovered = null;
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
    if (!state.ui) return;
    state.open = true;
    state.host.dataset.open = "true";
    state.ui.fab.dataset.expanded = "true";
    state.ui.fab.setAttribute("aria-expanded", "true");
    setStatus("", "");
    setProgress(0, true);
    renderSources();
    applyProgress();
    position();
    const target = state.ui.list.querySelector(".row-action") || state.ui.close;
    target.focus({ preventScroll: true });
  }

  function closePanel(focusFab) {
    const wasOpen = state.open;
    state.open = false;
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

      // A row that was handed over keeps its check while its download runs.
      if (track.url === url) {
        row.dataset.sent = "true";
        button.replaceChildren(bdmIcon("check"));
        button.setAttribute("aria-label", "Sent to B Download Manager");
      }

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

      startTracking(url, answer.result);
    } catch {
      setStatus("B Download Manager is not running", "error");
      button.disabled = false;
    } finally {
      state.sending = false;
    }
  }

  /* ------------------------------------------------------------- tracking */

  /** Follow the download a hand-over started, so the button can show it. */
  function startTracking(url, result) {
    stopTracking();
    track.url = url;
    track.id = Number.isInteger(result?.id) ? result.id : null;
    track.deadline = track.id === null ? Date.now() + WAIT_WINDOW : 0;
    applyProgress();
    schedulePoll(800);
  }

  /** Forget the download; nothing is followed. */
  function stopTracking() {
    if (track.timer) clearTimeout(track.timer);
    track.timer = 0;
    track.url = "";
    track.id = null;
    track.download = null;
    track.speed = "";
    track.sample = null;
    track.failures = 0;
    track.misses = 0;
    track.deadline = 0;
    track.done = false;
    applyProgress();
  }

  function schedulePoll(delay) {
    if (track.timer) clearTimeout(track.timer);
    track.timer = setTimeout(pollTracking, delay);
  }

  /**
   * Read the download behind the button.
   *
   * A hand-over that started at once answers with an id, so it is found by id.
   * One that asked is found by the address that was sent: the add dialog keeps
   * that address, and the list carries it, so nothing is guessed. The search
   * gives up after two minutes; a download can run for hours, so a known id is
   * followed for as long as the page lives.
   */
  async function pollTracking() {
    track.timer = 0;
    if (!track.url || track.done || !state.ready) return;

    if (document.hidden) {
      schedulePoll(POLL_HIDDEN);
      return;
    }

    let answer;
    try {
      answer = await browser.runtime.sendMessage({ type: "bdm:list" });
    } catch {
      answer = null;
    }
    if (!track.url || track.done) return;

    if (!answer || !answer.ok) {
      track.failures += 1;
      if (track.failures < 3) {
        schedulePoll(POLL_RUNNING);
        return;
      }
      track.download = { status: "unreachable" };
      track.done = true;
      applyProgress();
      return;
    }
    track.failures = 0;

    const downloads = Array.isArray(answer.downloads) ? answer.downloads : [];
    let download = track.id === null ? null : downloads.find((item) => item.id === track.id);
    if (!download && track.id === null) {
      // The newest unfinished download for the address that was handed over.
      download = downloads
        .filter((item) => item.url === track.url && !isFinished(item.status))
        .sort((a, b) => b.id - a.id)[0] || null;
      if (download) track.id = download.id;
    }

    if (!download) {
      // A known download that the application no longer lists was removed
      // there; there is nothing left to follow.
      if (track.id !== null) {
        track.misses += 1;
        if (track.misses > 5) {
          track.done = true;
          applyProgress();
          return;
        }
      } else if (track.deadline && Date.now() > track.deadline) {
        track.done = true;
        applyProgress();
        return;
      }
      if (track.id === null) applyProgress();
      schedulePoll(track.id === null ? POLL_WAITING : POLL_RUNNING);
      return;
    }

    track.misses = 0;
    track.speed = sampleSpeed(download);
    track.download = download;
    applyProgress();

    if (isFinished(download.status)) {
      track.done = true;
      return;
    }
    schedulePoll(POLL_RUNNING);
  }

  function isFinished(status) {
    return status === "completed" || status === "failed" || status === "cancelled";
  }

  function sampleSpeed(download) {
    const now = performance.now();
    const previous = track.sample;
    track.sample = { at: now, downloaded: download.downloaded };
    if (!previous) return "";
    const seconds = (now - previous.at) / 1000;
    const bytes = download.downloaded - previous.downloaded;
    if (seconds <= 0.15 || seconds > 12 || bytes <= 0) return "";
    return bdmFormatSpeed(bytes / seconds);
  }

  /** What the panel says about the tracked download. */
  function describeTrack(download) {
    if (!download) {
      // No download for the address yet: either the add dialog is waiting, or
      // it closed without starting anything.
      return {
        text: track.done ? "Not started in the app" : "Confirm it in the app",
        tone: "",
        percent: 0,
      };
    }
    if (download.status === "unreachable") {
      return { text: "B Download Manager is not running", tone: "error", percent: 0 };
    }
    if (download.status === "completed") {
      return { text: "Saved by B Download Manager", tone: "ok", percent: 100 };
    }
    if (download.status === "failed" || download.status === "cancelled") {
      return {
        text: download.error || "The app stopped this download",
        tone: "error",
        percent: bdmPercentOf(download),
      };
    }
    const percent = bdmPercentOf(download);
    if (download.status === "paused") {
      return { text: "Paused in the app", tone: "", percent };
    }
    return {
      text: track.speed ? `Downloading ${percent.toFixed(0)}% · ${track.speed}` : `Downloading ${percent.toFixed(0)}%`,
      tone: "ok",
      percent,
    };
  }

  /** Whether the tracked address is one of the sources the button offers. */
  function trackBelongsHere() {
    return Boolean(track.url) && state.sources.includes(track.url);
  }

  /**
   * Draw the tracked download on the button and, when it is open, the panel.
   *
   * The ring only appears on the video the download came from: the same button
   * on another video would claim a progress that is not its own.
   */
  function applyProgress() {
    const ui = state.ui;
    if (!ui) return;

    const here = trackBelongsHere();
    const download = here ? track.download : null;
    const shown = describeTrack(download);
    const done = download?.status === "completed";
    const broken =
      download?.status === "failed" ||
      download?.status === "cancelled" ||
      download?.status === "unreachable";
    const waiting = here && !download && !track.done;
    const percent = done ? 100 : shown.percent;

    // A pointer that crosses the player calls this often; only a change is
    // worth writing to the button.
    const stamp = `${here}|${shown.text}|${percent}|${state.open ? "open" : "closed"}`;
    if (stamp === state.progressStamp) return;
    state.progressStamp = stamp;

    const ring = done || broken || (here && !track.done);
    ui.fab.dataset.progress = ring ? "true" : "false";
    ui.fab.dataset.tone = done
      ? "success"
      : broken
        ? "error"
        : waiting
          ? "waiting"
          : here
            ? "running"
            : "";
    // A dashed waiting ring draws itself from the stylesheet; the others are
    // drawn from the percent here.
    ui.ringArc.style.strokeDashoffset = waiting
      ? ""
      : String(100 - clamp(percent, 0, 100));

    const name = ui.title.textContent;
    const glyph = done ? "check" : "download";
    if (ui.glyph.dataset.kind !== glyph) {
      ui.glyph.dataset.kind = glyph;
      ui.glyph.replaceChildren(bdmIcon(glyph));
    }

    if (here && (done || broken || download)) {
      ui.fab.setAttribute("aria-label", `${shown.text} — ${name}`);
      ui.fab.title = shown.text;
    } else if (waiting) {
      ui.fab.setAttribute("aria-label", `Waiting for B Download Manager — ${name}`);
      ui.fab.title = "Waiting for B Download Manager";
    } else if (name) {
      ui.fab.setAttribute("aria-label", `Download ${name} with B Download Manager`);
      ui.fab.title = `Download ${name}`;
    }

    if (!state.open || !here) return;
    setStatus(shown.text, shown.tone);
    setProgress(percent, false);
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

  /* ------------------------------------------------------------ following */

  /** Follow the videos of the page and the requests its players make. */
  function startFollowing() {
    if (state.stopRegistry) return;
    const stop = bdmStartVideoRegistry({ onChange: onPageVideosChanged });
    state.stopRegistry = typeof stop === "function" ? stop : null;
    bdmWatchMediaRequests(onMediaFound);
  }

  function stopFollowing() {
    state.stopRegistry?.();
    state.stopRegistry = null;
  }

  /** A player asked for another stream: the button may now have an address. */
  function onMediaFound() {
    scheduleCountRefresh();
    if (state.hovered && state.hovered.isConnected) present(state.hovered);
  }

  /** The page put a video in or took one out. */
  function onPageVideosChanged() {
    if (!state.ready || !overlayAllowed()) return;
    bdmCaptureSyncPage();
    scheduleCountRefresh();
    if (state.hovered && !state.hovered.isConnected) state.hovered = null;
    if (state.video && !state.video.isConnected) hide();
    if (state.hovered && state.hovered.isConnected) present(state.hovered);
  }

  /* ------------------------------------------------------------- events */

  function isOurEvent(path) {
    if (!state.host) return false;
    if (path.includes(state.host)) return true;
    return Boolean(state.root && path.includes(state.root));
  }

  function onPointerOver(event) {
    if (!state.ready || !overlayAllowed()) return;
    const path = event.composedPath?.() || [];
    if (isOurEvent(path)) {
      cancelHide();
      return;
    }

    const video = bdmVideoAtPoint(event.clientX, event.clientY, path, state.video);
    if (video) {
      cancelHide();
      present(video);
      return;
    }

    // The pointer left every video; nothing may claim the next stream that
    // arrives for it.
    state.hovered = null;
    if (state.open) closePanel(false);
    if (state.visible) scheduleHide();
  }

  /**
   * The pointer moved while the button was on its way out.
   *
   * A player puts its own gradient, title and controls above the picture, so
   * the pointer leaves the video element long before it leaves the player.
   * Comparing the point with the boxes keeps the button where it belongs.
   */
  function onPointerMove(event) {
    if (!state.visible || state.open || !state.hideTimer) return;
    if (!state.video) return;
    const rect = state.video.getBoundingClientRect();
    const inVideo = within(rect, event.clientX, event.clientY, POINTER_SLACK);
    const inFab = within(fabRect(), event.clientX, event.clientY, POINTER_SLACK);
    if (inVideo || inFab) cancelHide();
  }

  function onPointerDown(event) {
    if (!state.open) return;
    const path = event.composedPath?.() || [];
    if (isOurEvent(path)) return;
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
    const path = event.composedPath?.() || [];
    if (isOurEvent(path)) return;
    const video = bdmVideoInPath(path);
    if (video) {
      present(video);
    } else if (state.visible && !state.open) {
      scheduleHide();
    }
  }

  function onMediaChange(event) {
    const video = bdmIsVideo(event.target) ? event.target : null;
    if (!video) return;

    scheduleCountRefresh();
    if (video !== state.video && video !== state.hovered) return;
    present(video);
  }

  function onFullscreenChange() {
    if (!state.host) return;
    const mount = mountPoint();
    if (mount && state.host.parentNode !== mount) mount.appendChild(state.host);
    schedulePosition();
  }

  function onPageMoved() {
    if (!bdmCaptureSyncPage()) return;
    stopTracking();
    state.lastCount = -1;
    scheduleCountRefresh();
    if (state.hovered && state.hovered.isConnected) present(state.hovered);
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
        stopFollowing();
        if (wasAllowed || state.lastCount !== 0) sendCount(0);
        return;
      }

      if (!wasAllowed) {
        state.lastCount = -1;
        startFollowing();
        refreshCount();
      }
    });
  }

  function destroy() {
    hide();
    stopTracking();
    stopFollowing();
    removeHost();
    pageEvents.abort();
    state.sizeObserver?.disconnect();
    state.sizeObserver = null;
    state.observedVideo = null;
    browser.storage.onChanged.removeListener(onSettingsChanged);
    if (state.countTimer) clearTimeout(state.countTimer);
    if (state.positionFrame) cancelAnimationFrame(state.positionFrame);
    state.countTimer = 0;
    state.positionFrame = 0;
    state.ready = false;
  }

  /* --------------------------------------------------------------- start */

  document.addEventListener("pointerover", onPointerOver, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("pointerdown", onPointerDown, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("keydown", onKeyDown, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("focusin", onFocusIn, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("loadedmetadata", onMediaChange, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("durationchange", onMediaChange, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("emptied", onMediaChange, {
    capture: true,
    signal: pageEvents.signal,
  });
  document.addEventListener("fullscreenchange", onFullscreenChange, {
    signal: pageEvents.signal,
  });
  document.addEventListener(
    "visibilitychange",
    () => {
      if (document.hidden) hide();
    },
    { signal: pageEvents.signal },
  );
  window.addEventListener(
    "pagehide",
    (event) => {
      if (!event.persisted) destroy();
    },
    { signal: pageEvents.signal },
  );
  window.addEventListener("popstate", onPageMoved, { signal: pageEvents.signal });
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
      if (state.host) state.host.dataset.theme = settings.theme;
      if (overlayAllowed()) {
        startFollowing();
        refreshCount();
      } else {
        sendCount(0);
      }
    });
})();
