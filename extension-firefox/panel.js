/*
  The download panel IDM shows over a page: a small button in the corner of a
  page that has media, opening the list of files the page offers.

  A page cannot reach the bridge itself, because the bridge refuses any request
  whose origin is not the extension's. The hand-over therefore goes through the
  background page. The panel lives in a closed shadow root, so the page's own
  stylesheet cannot reach it, and carries a copy of the extension palette
  (theme.css) because a shadow root does not inherit the page's variables.
*/

(() => {
  if (window.top !== window) return;

  const MEDIA = /\.(mp4|m4v|webm|mkv|mov|avi|mp3|m4a|flac|wav|ogg|oga|opus|aac)(\?|#|$)/i;
  const MAX = 30;

  let files = [];
  let open = false;
  /* A closed panel stays closed for this page view only; a reload shows it. */
  let dismissed = false;
  let enabled = true;
  let ready = false;
  let lastCount = -1;
  let host = null;
  let ui = null;

  function nameOfLink(url) {
    try {
      const last = new URL(url).pathname.split("/").filter(Boolean).pop();
      return last ? decodeURIComponent(last) : "";
    } catch {
      return "";
    }
  }

  /* Only links the application can actually fetch. A blob: stream from MSE has
     no address outside the tab, so it is not offered. */
  function collect() {
    const found = new Set();
    const add = (value) => {
      if (!value) return;
      try {
        const url = new URL(value, location.href);
        if (url.protocol === "http:" || url.protocol === "https:") found.add(url.href);
      } catch {
        // A malformed address is not worth showing.
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

    return [...found].slice(0, MAX);
  }

  const STYLE = `
    :host { color-scheme: dark; }
    @media (prefers-color-scheme: light) { :host { color-scheme: light; } }

    *, *::before, *::after { box-sizing: border-box; }
    [hidden] { display: none !important; }

    .panel {
      --canvas: #03080a;
      --text: #d7e3df;
      --text-muted: #869a93;
      --line: #13312a;
      --hover: #081512;
      --accent: #c8ff00;
      --on-accent: #022c1e;
      --focus: #b4e34f;
      --ok: #2fd06b;

      font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
      font-size: 0.8125rem;
      line-height: 1.4;
      color: var(--text);
    }

    @media (prefers-color-scheme: light) {
      .panel {
        --canvas: #ffffff;
        --text: #17181b;
        --text-muted: #565b64;
        --line: #d8dade;
        --hover: #f0f1f4;
        --accent: #022c1e;
        --on-accent: #c8ff00;
        --focus: #4d7a12;
        --ok: #1a7f37;
      }
    }

    .trigger {
      display: inline-flex;
      align-items: center;
      gap: 0.375rem;
      margin: 0;
      padding: 0.5rem 0.625rem 0.5rem 0.5rem;
      border: 1px solid var(--line);
      border-radius: 0.5rem;
      background: var(--canvas);
      color: var(--text);
      font: inherit;
      font-size: 0.75rem;
      font-weight: 500;
      line-height: 1;
      cursor: pointer;
      box-shadow: 0 6px 18px rgb(0 0 0 / 0.28);
      transition: background-color 120ms ease, border-color 120ms ease;
    }

    .trigger:hover { background: var(--hover); border-color: var(--text-muted); }
    .trigger:active { background: var(--hover); }
    .trigger:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }

    .trigger .icon { flex: none; width: 0.875rem; height: 0.875rem; }

    .count {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-width: 1rem;
      height: 1rem;
      padding: 0 0.25rem;
      border-radius: 999px;
      background: var(--accent);
      color: var(--on-accent);
      font-size: 0.625rem;
      font-variant-numeric: tabular-nums;
    }

    .sheet {
      display: flex;
      flex-direction: column;
      width: min(20rem, calc(100vw - 2rem));
      max-height: min(24rem, 70vh);
      overflow: hidden;
      border: 1px solid var(--line);
      border-radius: 0.625rem;
      background: var(--canvas);
      color: var(--text);
      box-shadow: 0 12px 32px rgb(0 0 0 / 0.35);
    }

    .sheet:focus { outline: none; }

    .head {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      padding: 0.5rem 0.5rem 0.5rem 0.75rem;
      border-block-end: 1px solid var(--line);
    }

    .head h2 { flex: 1; margin: 0; font-size: 0.75rem; font-weight: 600; }

    .close {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 1.75rem;
      height: 1.75rem;
      padding: 0;
      border: 0;
      border-radius: 0.375rem;
      background: transparent;
      color: var(--text-muted);
      cursor: pointer;
    }

    .close:hover { background: var(--hover); color: var(--text); }
    .close:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
    .close svg { width: 0.875rem; height: 0.875rem; }

    .rows { margin: 0; padding: 0; overflow-y: auto; list-style: none; }

    .row {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      padding: 0.375rem 0.75rem;
    }

    .row + .row { border-block-start: 1px solid var(--line); }

    .name {
      flex: 1;
      min-width: 0;
      overflow: hidden;
      font-size: 0.75rem;
      white-space: nowrap;
      text-overflow: ellipsis;
    }

    .row-button {
      flex: none;
      padding: 0.25rem 0.5rem;
      border: 1px solid var(--line);
      border-radius: 0.375rem;
      background: transparent;
      color: var(--text);
      font: inherit;
      font-size: 0.6875rem;
      line-height: 1.4;
      cursor: pointer;
    }

    .row-button:hover:not(:disabled) { background: var(--hover); }
    .row-button:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
    .row-button:disabled { cursor: default; color: var(--text-muted); }
    .row-button[data-sent="true"] { border-color: var(--ok); color: var(--ok); }

    .note { margin: 0; padding: 0 0.75rem 0.5rem; color: var(--text-muted); font-size: 0.6875rem; }
    .note:empty { display: none; }

    @media (prefers-reduced-motion: reduce) { .trigger { transition: none; } }
  `;

  const MARKUP = `
    <div class="panel">
      <button class="trigger" type="button" aria-expanded="false" aria-controls="idm-panel-sheet">
        <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M12 3v12" /><path d="m7 10 5 5 5-5" /><path d="M5 21h14" />
        </svg>
        <span>Download</span>
        <span class="count" aria-hidden="true">0</span>
      </button>

      <section class="sheet" id="idm-panel-sheet" tabindex="-1"
               aria-label="Download with B Download Manager" hidden>
        <header class="head">
          <h2>Download with B Download Manager</h2>
          <button class="close" type="button" aria-label="Hide the download panel">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M18 6 6 18" /><path d="m6 6 12 12" />
            </svg>
          </button>
        </header>
        <ul class="rows"></ul>
        <p class="note" role="status" aria-live="polite"></p>
      </section>
    </div>
  `;

  function mount() {
    if (host || !enabled) return;

    host = document.createElement("div");
    host.setAttribute("data-idm-panel", "");
    host.style.cssText =
      "all: initial; position: fixed; inset-block-end: 1rem; inset-inline-end: 1rem; z-index: 2147483647;";

    const root = host.attachShadow({ mode: "closed" });
    /* A constructed stylesheet is not a style element, so a page's CSP cannot
       block it, and the page cannot reach it either. */
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(STYLE);
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = MARKUP;
    (document.body || document.documentElement).append(host);

    ui = {
      trigger: root.querySelector(".trigger"),
      count: root.querySelector(".count"),
      sheet: root.querySelector(".sheet"),
      close: root.querySelector(".close"),
      rows: root.querySelector(".rows"),
      note: root.querySelector(".note"),
    };

    ui.trigger.addEventListener("click", () => setOpen(true));
    ui.close.addEventListener("click", () => {
      dismissed = true;
      setOpen(false);
    });
  }

  function unmount() {
    host?.remove();
    host = null;
    ui = null;
    open = false;
  }

  function update() {
    if (!ui) return;
    ui.trigger.hidden = dismissed || open || files.length === 0;
    ui.sheet.hidden = !open;
    ui.trigger.setAttribute("aria-expanded", String(open));
    ui.count.textContent = files.length > 9 ? "9+" : String(files.length);
    ui.trigger.setAttribute(
      "aria-label",
      `Download with B Download Manager, ${files.length} ${
        files.length === 1 ? "file" : "files"
      } on this page`,
    );
  }

  function render() {
    if (!ui) return;
    ui.rows.textContent = "";
    ui.note.textContent = "";

    for (const url of files) {
      const row = document.createElement("li");
      row.className = "row";

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = nameOfLink(url) || url;
      name.title = url;

      const button = document.createElement("button");
      button.type = "button";
      button.className = "row-button";
      button.textContent = "Download";
      button.setAttribute("aria-label", `Download ${name.textContent}`);
      button.addEventListener("click", () => send(url, button));

      row.append(name, button);
      ui.rows.append(row);
    }
  }

  function setOpen(next) {
    open = Boolean(next) && files.length > 0 && Boolean(ui);
    if (open) render();
    update();
    if (open) ui.sheet.focus();
  }

  async function send(url, button) {
    button.disabled = true;
    button.textContent = "Sending";
    ui.note.textContent = "";

    let ok = false;
    try {
      const answer = await browser.runtime.sendMessage({
        type: "download",
        url,
        name: nameOfLink(url),
        referer: location.href,
      });
      ok = Boolean(answer?.ok);
    } catch {
      ok = false;
    }

    if (ok) {
      button.textContent = "Sent";
      button.dataset.sent = "true";
      ui.note.textContent = "The application asked where to save it.";
    } else {
      button.disabled = false;
      button.textContent = "Try again";
      ui.note.textContent = "The application is not running.";
    }
  }

  function refresh() {
    if (!ready) return;

    /* With the button off, the badge goes too, so the toolbar does not point at
       a panel that was turned off. */
    if (!enabled) {
      if (lastCount !== 0) {
        lastCount = 0;
        browser.runtime.sendMessage({ type: "media-count", count: 0 }).catch(() => {});
      }
      return;
    }

    const next = collect();
    const changed =
      next.length !== files.length || next.some((url, index) => url !== files[index]);
    files = next;

    if (files.length && enabled && !dismissed) mount();
    if (open && changed) render();
    if (!files.length) open = false;
    update();

    if (files.length !== lastCount) {
      lastCount = files.length;
      browser.runtime.sendMessage({ type: "media-count", count: lastCount }).catch(() => {});
    }
  }

  async function applySettings() {
    const settings = await loadSettings();
    enabled = settings.panel;
    ready = true;

    if (!enabled) {
      dismissed = false;
      unmount();
    }

    refresh();
  }

  applySettings();

  /* Media that arrives after the page has loaded: a player swaps its source, an
     SPA adds a video. The debounce keeps a busy page from being scanned on
     every node. */
  let pending = 0;
  new MutationObserver(() => {
    clearTimeout(pending);
    pending = setTimeout(refresh, 400);
  }).observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["src"],
  });

  document.addEventListener("loadedmetadata", refresh, true);

  document.addEventListener(
    "pointerdown",
    (event) => {
      if (open && !event.composedPath().includes(host)) setOpen(false);
    },
    true,
  );

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !open) return;
    setOpen(false);
    ui?.trigger.focus();
  });

  browser.storage.onChanged.addListener((_changes, area) => {
    if (area === "local") applySettings();
  });
})();
