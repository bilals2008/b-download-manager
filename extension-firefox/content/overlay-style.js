/*
  The look of the floating button and its panel.

  A shadow root does not inherit the page, and a content script cannot read the
  application's `src/index.css`, so the application's palette is repeated here
  with the same token names. The values are the dark and light blocks of
  `src/index.css`; `scripts/check-extension.mjs` asserts that the shared
  colours still match `theme.css`.
*/

const BDM_OVERLAY_STYLE = `
  :host {
    all: initial;
    color-scheme: dark;

    --bdm-canvas: #0b1416;
    --bdm-text: #d7e3df;
    --bdm-muted: #869a93;
    --bdm-line: #13312a;
    --bdm-hover: #11201f;
    --bdm-active: #16302b;
    --bdm-primary: #022c1e;
    --bdm-on-primary: #c8ff00;
    --bdm-progress: #c8ff00;
    --bdm-success: #2fd06b;
    --bdm-danger: #ff9d90;
    --bdm-focus: #b4e34f;
    --bdm-shadow:
      0 18px 40px -18px rgb(0 0 0 / 0.75),
      0 4px 12px -6px rgb(0 0 0 / 0.5);

    --bdm-radius: 0.75rem;
    --bdm-radius-control: 0.5rem;
    --bdm-font:
      ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  }

  :host([data-theme="light"]) {
    color-scheme: light;
    --bdm-canvas: #ffffff;
    --bdm-text: #17181b;
    --bdm-muted: #565b64;
    --bdm-line: #d8dade;
    --bdm-hover: #e9eaee;
    --bdm-active: #dcdfe5;
    --bdm-primary: #022c1e;
    --bdm-on-primary: #c8ff00;
    --bdm-progress: #022c1e;
    --bdm-success: #1a7f37;
    --bdm-danger: #b3261e;
    --bdm-focus: #4d7a12;
    --bdm-shadow:
      0 18px 40px -18px rgb(15 23 42 / 0.3),
      0 4px 12px -6px rgb(15 23 42 / 0.18);
  }

  @media (prefers-color-scheme: light) {
    :host([data-theme="auto"]) {
      color-scheme: light;
      --bdm-canvas: #ffffff;
      --bdm-text: #17181b;
      --bdm-muted: #565b64;
      --bdm-line: #d8dade;
      --bdm-hover: #e9eaee;
      --bdm-active: #dcdfe5;
      --bdm-primary: #022c1e;
      --bdm-on-primary: #c8ff00;
      --bdm-progress: #022c1e;
      --bdm-success: #1a7f37;
      --bdm-danger: #b3261e;
      --bdm-focus: #4d7a12;
      --bdm-shadow:
        0 18px 40px -18px rgb(15 23 42 / 0.3),
        0 4px 12px -6px rgb(15 23 42 / 0.18);
    }
  }

  *, *::before, *::after { box-sizing: border-box; }
  [hidden] { display: none !important; }

  .fab {
    position: absolute;
    left: 0;
    top: 0;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    margin: 0;
    padding: 0;
    border: 1px solid var(--bdm-line);
    border-radius: 9px;
    color: var(--bdm-text);
    background: color-mix(in srgb, var(--bdm-canvas) 86%, transparent);
    box-shadow: var(--bdm-shadow);
    -webkit-backdrop-filter: blur(14px) saturate(140%);
    backdrop-filter: blur(14px) saturate(140%);
    cursor: pointer;
    opacity: 0;
    visibility: hidden;
    transform: translateY(-3px) scale(0.94);
    transition:
      opacity 140ms ease,
      transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1),
      visibility 180ms,
      background-color 120ms ease,
      border-color 120ms ease;
  }

  :host([data-visible="true"]) .fab {
    opacity: 1;
    visibility: visible;
    transform: none;
  }

  .fab:hover {
    background: var(--bdm-hover);
    border-color: color-mix(in srgb, var(--bdm-line) 45%, var(--bdm-muted));
  }

  .fab:active { background: var(--bdm-active); }

  .fab:focus-visible {
    outline: 2px solid var(--bdm-focus);
    outline-offset: 2px;
  }

  .fab .glyph {
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
  }

  .fab .glyph svg {
    width: 15px;
    height: 15px;
    color: var(--bdm-progress);
    transition: transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  .fab:hover .glyph svg { transform: translateY(1px); }

  /*
    The ring is the download behind the button: an arc for how far it got, a
    full circle for a finished file, a dashed one while the application still
    has its add dialog open. It only appears on the video it belongs to.
  */
  .ring {
    position: absolute;
    inset: -4px;
    width: 38px;
    height: 38px;
    overflow: visible;
    transform: rotate(-90deg);
    opacity: 0;
    pointer-events: none;
    transition: opacity 160ms ease;
  }

  .fab[data-progress="true"] .ring { opacity: 1; }

  .ring circle {
    fill: none;
    stroke-width: 2;
  }

  .ring-track { stroke: color-mix(in srgb, var(--bdm-line) 80%, transparent); }

  .ring-arc {
    stroke: var(--bdm-progress);
    stroke-linecap: round;
    /* The circle's pathLength makes these 100 units the whole way round. */
    stroke-dasharray: 100 100;
    stroke-dashoffset: 100;
    transition: stroke-dashoffset 320ms ease, stroke 160ms ease;
  }

  .fab[data-tone="waiting"] .ring-arc {
    stroke: var(--bdm-muted);
    stroke-dasharray: 6 6;
    stroke-dashoffset: 0;
  }

  .fab[data-tone="success"] .ring-arc { stroke: var(--bdm-success); }
  .fab[data-tone="success"] .glyph svg { color: var(--bdm-success); }

  .fab[data-tone="error"] .ring-arc { stroke: var(--bdm-danger); }
  .fab[data-tone="error"] .glyph svg { color: var(--bdm-danger); }

  .fab[data-expanded="true"] {
    background: var(--bdm-active);
    border-color: color-mix(in srgb, var(--bdm-line) 45%, var(--bdm-muted));
  }

  .panel {
    position: absolute;
    display: flex;
    flex-direction: column;
    width: 260px;
    max-height: min(340px, calc(100vh - 24px));
    overflow: hidden;
    border: 1px solid var(--bdm-line);
    border-radius: var(--bdm-radius);
    color: var(--bdm-text);
    background: color-mix(in srgb, var(--bdm-canvas) 92%, transparent);
    box-shadow: var(--bdm-shadow);
    -webkit-backdrop-filter: blur(20px) saturate(150%);
    backdrop-filter: blur(20px) saturate(150%);
    font-family: var(--bdm-font);
    font-size: 12px;
    line-height: 1.35;
    letter-spacing: 0;
    text-align: start;
    opacity: 0;
    visibility: hidden;
    transform: translateY(-4px) scale(0.985);
    transform-origin: top right;
    transition:
      opacity 140ms ease,
      transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1),
      visibility 180ms;
  }

  :host([data-open="true"]) .panel {
    opacity: 1;
    visibility: visible;
    transform: none;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 8px 8px 12px;
    border-block-end: 1px solid var(--bdm-line);
  }

  .head-text { flex: 1; min-width: 0; }

  .title {
    display: block;
    overflow: hidden;
    color: var(--bdm-text);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .caption {
    display: block;
    overflow: hidden;
    margin-block-start: 1px;
    color: var(--bdm-muted);
    font-size: 10px;
    font-weight: 500;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .close {
    display: grid;
    flex: none;
    place-items: center;
    width: 22px;
    height: 22px;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 6px;
    color: var(--bdm-muted);
    background: transparent;
    cursor: pointer;
  }

  .close:hover { color: var(--bdm-text); background: var(--bdm-hover); }

  .close:focus-visible {
    outline: 2px solid var(--bdm-focus);
    outline-offset: 1px;
  }

  .close svg { width: 12px; height: 12px; }

  .list {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
    scrollbar-width: thin;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 4px 4px 8px;
    border-radius: 7px;
  }

  .row:hover { background: var(--bdm-hover); }

  .tag {
    flex: none;
    padding: 1px 4px;
    border: 1px solid var(--bdm-line);
    border-radius: 4px;
    color: var(--bdm-muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--bdm-text);
    font-size: 11px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .row-action {
    display: grid;
    flex: none;
    place-items: center;
    width: 24px;
    height: 24px;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 6px;
    color: var(--bdm-muted);
    background: transparent;
    cursor: pointer;
  }

  .row-action:hover { color: var(--bdm-progress); background: var(--bdm-active); }

  .row-action:focus-visible {
    outline: 2px solid var(--bdm-focus);
    outline-offset: 1px;
  }

  .row-action:disabled { cursor: default; opacity: 0.55; }

  .row-action svg { width: 13px; height: 13px; }

  .row[data-sent="true"] .row-action { color: var(--bdm-success); }

  .empty {
    margin: 0;
    padding: 12px;
    color: var(--bdm-muted);
    font-size: 11px;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 5px 6px 5px 12px;
    border-block-start: 1px solid var(--bdm-line);
  }

  .status {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--bdm-muted);
    font-size: 10px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .status[data-tone="ok"] { color: var(--bdm-success); }
  .status[data-tone="error"] { color: var(--bdm-danger); }

  .text-button {
    flex: none;
    margin: 0;
    padding: 2px 5px;
    border: 0;
    border-radius: 5px;
    color: var(--bdm-muted);
    background: transparent;
    font: inherit;
    font-size: 10px;
    cursor: pointer;
  }

  .text-button:hover { color: var(--bdm-text); background: var(--bdm-hover); }

  .text-button:focus-visible {
    outline: 2px solid var(--bdm-focus);
    outline-offset: 1px;
  }

  .bar {
    height: 2px;
    background: var(--bdm-line);
  }

  .bar > span {
    display: block;
    width: 0;
    height: 100%;
    background: var(--bdm-progress);
    transition: width 320ms ease;
  }

  @media (prefers-reduced-motion: reduce) {
    .fab,
    .panel,
    .fab .glyph svg,
    .ring,
    .ring-arc,
    .bar > span {
      transition: none;
    }
  }

  @media (forced-colors: active) {
    .fab,
    .panel {
      border: 1px solid CanvasText;
      background: Canvas;
      color: CanvasText;
      box-shadow: none;
    }

    .tag,
    .close,
    .row-action,
    .text-button {
      color: LinkText;
      forced-color-adjust: none;
    }

    .ring-track { stroke: CanvasText; }
    .ring-arc { stroke: Highlight; }
  }
`;
