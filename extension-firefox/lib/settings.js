/*
  The preferences the popup, the settings page and the auto-save share.

  Loaded before the other scripts in each context, so the functions below are
  plain globals. Nothing here touches the page: the content script gets the
  same settings but writes its own shadow root, never the page's styles.
*/

const BDM_DEFAULT_SETTINGS = Object.freeze({
  /* Take over the downloads the browser starts on its own. */
  intercept: true,
  /* Offer the floating button on a video that can be downloaded. */
  overlay: true,
  /* Open the app's add dialog instead of starting the download at once. */
  ask: false,
  /* File types to take over. Empty means every type. */
  types: [],
  /* Hosts the browser keeps, with their subdomains. */
  skipSites: [],
  /* auto, light or dark. */
  theme: "auto",
});

/** ".MP4", "*.mp4" and " mp4 " are all the same extension. */
function bdmCleanType(entry) {
  return String(entry || "")
    .trim()
    .toLowerCase()
    .replace(/^\*?\./, "");
}

/** A host name out of anything the user may have typed. Junk becomes "". */
const BDM_HOST = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/;

function bdmCleanSite(entry) {
  const text = String(entry || "")
    .trim()
    .replace(/^\*?\./, "");
  if (!text) return "";
  try {
    const host = new URL(text.includes("://") ? text : `http://${text}`).hostname.toLowerCase();
    if (host === "localhost") return host;
    return BDM_HOST.test(host) ? host : "";
  } catch {
    return "";
  }
}

/** The clean settings, with the old names of the two switches understood. */
function bdmNormalizeSettings(raw) {
  const source = raw && typeof raw === "object" ? raw : {};
  const list = (value) =>
    (Array.isArray(value) ? value : [])
      .map((entry) => String(entry).trim().toLowerCase())
      .filter(Boolean);

  // Before 1.0 the switches were called takeover and panel.
  const intercept =
    typeof source.intercept === "boolean"
      ? source.intercept
      : source.takeover !== false;
  const overlay =
    typeof source.overlay === "boolean" ? source.overlay : source.panel !== false;

  return {
    intercept,
    overlay,
    ask: source.ask === true,
    types: [...new Set(list(source.types).map(bdmCleanType).filter(Boolean))],
    skipSites: [...new Set(list(source.skipSites).map(bdmCleanSite).filter(Boolean))],
    theme:
      source.theme === "light" || source.theme === "dark"
        ? source.theme
        : "auto",
  };
}

async function bdmLoadSettings() {
  const raw = await browser.storage.local.get(BDM_DEFAULT_SETTINGS);
  return bdmNormalizeSettings(raw);
}

async function bdmSaveSettings(settings) {
  const clean = bdmNormalizeSettings(settings);
  await browser.storage.local.set(clean);
  return clean;
}

/** The extension of a link, lower case, without the dot. */
function bdmExtensionOf(url) {
  try {
    const name = new URL(url).pathname.split("/").filter(Boolean).pop() || "";
    const index = name.lastIndexOf(".");
    return index > 0 ? name.slice(index + 1).toLowerCase() : "";
  } catch {
    return "";
  }
}

/** A host the settings asked to leave alone, or one of its subdomains. */
function bdmIsSkipped(host, skipSites) {
  if (!host) return false;
  return skipSites.some((entry) => host === entry || host.endsWith(`.${entry}`));
}

/** Whether the floating button may appear on a page of this host. */
function bdmAllowsOverlay(settings, host) {
  return Boolean(settings.overlay) && !bdmIsSkipped(String(host || "").toLowerCase(), settings.skipSites);
}

/** The host of an address, or "" when it does not have one. */
function bdmHostOf(url) {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return "";
  }
}

/*
  Whether the application should take a download over. A skip rule follows the
  page the file was linked from; a download with no referrer falls back to the
  host of the file itself. An explicit click never comes here.
*/
function bdmShouldIntercept(settings, url, pageUrl) {
  if (!settings.intercept) return false;
  if (!/^(https?|ftp):/.test(url)) return false;
  if (settings.types.length && !settings.types.includes(bdmExtensionOf(url))) return false;
  if (bdmIsSkipped(bdmCleanSite(pageUrl) || bdmHostOf(url), settings.skipSites)) return false;
  return true;
}

/** Auto, light or dark as a value the stylesheets can select on. */
function bdmThemeName(theme) {
  return theme === "light" || theme === "dark" ? theme : "auto";
}

/** Apply the chosen theme to an extension page. */
function bdmApplyPageTheme(theme, root) {
  const element = root || document.documentElement;
  if (element) element.dataset.bdmTheme = bdmThemeName(theme);
}

/**
 * The token names an app palette uses, per surface.
 *
 * The shadow-root panel and the extension pages name the same meanings
 * differently; the application sends meanings, so one table per surface is
 * enough and a new theme needs no change here.
 */
const BDM_OVERLAY_TOKENS = {
  canvas: "--bdm-canvas",
  text: "--bdm-text",
  muted: "--bdm-muted",
  line: "--bdm-line",
  hover: "--bdm-hover",
  active: "--bdm-active",
  primary: "--bdm-primary",
  "on-primary": "--bdm-on-primary",
  progress: "--bdm-progress",
  success: "--bdm-success",
  danger: "--bdm-danger",
  focus: "--bdm-focus",
  radius: "--bdm-radius",
  shadow: "--bdm-shadow",
};

const BDM_PAGE_TOKENS = {
  canvas: "--canvas",
  surface: "--surface",
  text: "--text",
  muted: "--text-muted",
  line: "--line",
  hover: "--hover",
  active: "--active",
  primary: "--primary",
  "on-primary": "--on-primary",
  progress: "--progress",
  success: "--success",
  danger: "--danger",
  focus: "--focus",
  radius: "--radius",
  shadow: "--shadow",
};

/** Paint one surface in the app's tokens. Missing tokens keep the local value. */
function bdmApplyAppPalette(appTheme, root, overlay) {
  if (!appTheme || !appTheme.palette || !root) return false;
  const tokens = overlay ? BDM_OVERLAY_TOKENS : BDM_PAGE_TOKENS;
  for (const [meaning, token] of Object.entries(tokens)) {
    const value = appTheme.palette[meaning];
    if (value) root.style.setProperty(token, value);
  }
  const accent = appTheme.palette.progress || appTheme.palette.primary;
  if (!overlay) {
    if (accent) root.style.setProperty("--accent", accent);
    if (appTheme.palette["on-primary"]) {
      root.style.setProperty("--on-accent", appTheme.palette["on-primary"]);
    }
  }
  if (typeof appTheme.light === "boolean") {
    const name = appTheme.light ? "light" : "dark";
    root.style.setProperty("color-scheme", name);
    if (overlay) root.dataset.theme = name;
    else root.dataset.bdmTheme = name;
  }
  return true;
}

/**
 * Ask the application for the appearance it is drawn in and paint `root`.
 *
 * The extension's own theme setting is used until the app answers; a running
 * application owns the palette, so a custom theme reaches the panel too.
 */
async function bdmSyncAppTheme(root, overlay) {
  if (typeof browser === "undefined" || !browser.runtime?.sendMessage) {
    return false;
  }
  try {
    const answer = await browser.runtime.sendMessage({ type: "bdm:theme" });
    if (!answer || !answer.ok || !answer.theme) return false;
    return bdmApplyAppPalette(answer.theme, root, overlay);
  } catch {
    return false;
  }
}
