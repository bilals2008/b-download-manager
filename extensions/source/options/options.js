/* The settings page. Every change is written as it is made; the page has no
   Save button and nothing to cancel. */

const switches = {
  intercept: document.getElementById("intercept"),
  ask: document.getElementById("ask"),
  overlay: document.getElementById("overlay"),
};

const themeGroup = document.querySelector(".segmented");
const themeButtons = [...document.querySelectorAll(".segmented [data-theme]")];
const typesList = document.getElementById("types");
const typesEmpty = document.getElementById("types-empty");
const typeInput = document.getElementById("type-input");
const typeNote = document.getElementById("type-note");
const sitesList = document.getElementById("sites");
const sitesEmpty = document.getElementById("sites-empty");
const siteInput = document.getElementById("site-input");
const siteNote = document.getElementById("site-note");
const status = document.getElementById("status");
const statusText = document.getElementById("status-text");
const retry = document.getElementById("retry");
const saved = document.getElementById("saved");
const resetButton = document.getElementById("reset");

const SAVE_DELAY = 300;
const STATUS_POLL = 5000;

let settings = bdmNormalizeSettings({});
let saveTimer = 0;
let saving = 0;
let savedTimer = 0;
let statusTimer = 0;
let resetTimer = 0;
let resetArmed = false;
let checking = false;
/** Whether the application answered as the extension. */
let connected = false;

/* ----------------------------------------------------------------- render */

function apply() {
  for (const [key, element] of Object.entries(switches)) {
    element.setAttribute("aria-checked", settings[key] ? "true" : "false");
  }

  for (const button of themeButtons) {
    const checked = button.dataset.theme === settings.theme;
    button.setAttribute("aria-checked", checked ? "true" : "false");
    // A radio group has one tab stop; the arrow keys move within it.
    button.tabIndex = checked ? 0 : -1;
  }

  renderList(typesList, typesEmpty, settings.types, "type");
  renderList(sitesList, sitesEmpty, settings.skipSites, "site");
}

function renderList(container, empty, values, kind) {
  container.textContent = "";
  empty.hidden = values.length > 0;

  for (const value of values) {
    const chip = document.createElement("span");
    chip.className = "chip";

    const text = document.createElement("span");
    text.className = "chip-text";
    text.textContent = kind === "type" ? `.${value}` : value;

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "chip-remove";
    remove.setAttribute("aria-label", `Remove ${text.textContent}`);
    remove.title = `Remove ${text.textContent}`;
    remove.append(bdmIcon("close"));
    remove.addEventListener("click", () => {
      const key = kind === "type" ? "types" : "skipSites";
      settings = {
        ...settings,
        [key]: settings[key].filter((entry) => entry !== value),
      };
      apply();
      scheduleSave();
      (kind === "type" ? typeInput : siteInput).focus();
    });

    chip.append(text, remove);
    container.append(chip);
  }
}

/* ------------------------------------------------------------------- save */

function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = 0;
    void save();
  }, SAVE_DELAY);
}

async function save() {
  const snapshot = settings;
  saving += 1;
  try {
    const clean = await bdmSaveSettings(snapshot);
    // A change made while the write was in flight owns the newer state; its
    // own timer saves it.
    if (settings === snapshot) settings = clean;
    await pushRulesToApp(clean);
    saved.textContent = "Saved";
    clearTimeout(savedTimer);
    savedTimer = setTimeout(() => {
      saved.textContent = "";
    }, 1600);
  } catch {
    saved.textContent = "Not saved";
  } finally {
    saving -= 1;
  }
}

/**
 * Give the four capture rules to the app.
 *
 * While the app runs, its Settings page owns them, so a change here that
 * stayed local would be overwritten by the app's copy on the next hand-over.
 * A closed app cannot take them; the local copy is the fallback for exactly
 * that case.
 */
async function pushRulesToApp(next) {
  try {
    return await browser.runtime.sendMessage({
      type: "bdm:capture-save",
      policy: {
        capture: next.intercept,
        ask: next.ask,
        types: [...next.types],
        skipSites: [...next.skipSites],
      },
    });
  } catch {
    return null;
  }
}

/**
 * Open on the rules the app is really using.
 *
 * The page is redrawn from the app's answer whenever it becomes visible, so a
 * switch turned in the app is shown here without a reload. A local change
 * that has not reached storage yet is left alone.
 */
async function adoptAppRules() {
  if (saveTimer || saving) return;
  const answer = await browser.runtime
    .sendMessage({ type: "bdm:capture" })
    .catch(() => null);
  if (!answer?.ok || !answer.policy) return;
  const policy = answer.policy;
  const next = bdmNormalizeSettings({
    ...settings,
    intercept: policy.capture === true,
    ask: policy.ask === true,
    types: Array.isArray(policy.types) ? policy.types : settings.types,
    skipSites: Array.isArray(policy.skipSites) ? policy.skipSites : settings.skipSites,
  });
  if (JSON.stringify(next) === JSON.stringify(settings)) return;
  settings = next;
  apply();
  await bdmSaveSettings(next).catch(() => {});
}

/* ------------------------------------------------------------------- add */

function note(element, message) {
  element.textContent = message;
}

function addType() {
  const clean = bdmCleanType(typeInput.value);
  if (!clean || !/^[a-z0-9][a-z0-9+_-]{0,15}$/.test(clean)) {
    note(typeNote, "Use a short extension, like mp4.");
    return;
  }
  if (settings.types.includes(clean)) {
    note(typeNote, "That type is already on the list.");
    typeInput.value = "";
    return;
  }

  settings = { ...settings, types: [...settings.types, clean] };
  typeInput.value = "";
  note(typeNote, "");
  apply();
  scheduleSave();
  typeInput.focus();
}

function addSite() {
  const clean = bdmCleanSite(siteInput.value);
  if (!clean) {
    note(siteNote, "Enter a site like example.org.");
    return;
  }
  if (settings.skipSites.includes(clean)) {
    note(siteNote, "That site is already on the list.");
    siteInput.value = "";
    return;
  }

  settings = { ...settings, skipSites: [...settings.skipSites, clean] };
  siteInput.value = "";
  note(siteNote, "");
  apply();
  scheduleSave();
  siteInput.focus();
}

/* -------------------------------------------------------------- switches */

for (const [key, element] of Object.entries(switches)) {
  element.addEventListener("click", () => {
    settings = { ...settings, [key]: !settings[key] };
    apply();
    scheduleSave();
  });
}

for (const button of themeButtons) {
  button.addEventListener("click", () => {
    const theme = button.dataset.theme;
    if (theme !== "auto" && theme !== "light" && theme !== "dark") return;
    settings = { ...settings, theme };
    bdmApplyPageTheme(theme);
    apply();
    scheduleSave();
  });
}

themeGroup.addEventListener("keydown", (event) => {
  const step =
    event.key === "ArrowRight" || event.key === "ArrowDown"
      ? 1
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? -1
        : 0;
  if (!step && event.key !== "Home" && event.key !== "End") return;

  const current = themeButtons.indexOf(document.activeElement);
  if (current < 0) return;
  event.preventDefault();

  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? themeButtons.length - 1
        : (current + step + themeButtons.length) % themeButtons.length;
  themeButtons[next].focus();
  themeButtons[next].click();
});

/* ------------------------------------------------------------------ reset */

function disarmReset() {
  resetArmed = false;
  clearTimeout(resetTimer);
  resetButton.textContent = "Reset to defaults";
  resetButton.classList.remove("confirming");
}

resetButton.addEventListener("click", async () => {
  if (!resetArmed) {
    resetArmed = true;
    resetButton.textContent = "Reset everything?";
    resetButton.classList.add("confirming");
    resetTimer = setTimeout(disarmReset, 4000);
    return;
  }

  disarmReset();
  clearTimeout(saveTimer);
  saveTimer = 0;
  settings = await bdmSaveSettings(BDM_DEFAULT_SETTINGS);
  await pushRulesToApp(settings);
  bdmApplyPageTheme(settings.theme);
  apply();
  saved.textContent = "Reset";
  clearTimeout(savedTimer);
  savedTimer = setTimeout(() => {
    saved.textContent = "";
  }, 1600);
});

/* ------------------------------------------------------------------- app */

function setStatus(state, text) {
  status.dataset.state = state;
  statusText.textContent = text;
  retry.hidden = state !== "offline";
}

/**
 * One line about the connection, in the reader's terms.
 *
 * There is nothing to pair: a reachable app is a working integration. The two
 * states the reader has to tell apart are "Not running" and "Connected".
 */
async function checkApp() {
  if (checking) return;
  checking = true;
  const answer = await browser.runtime
    .sendMessage({ type: "bdm:status" })
    .catch(() => null);
  checking = false;

  if (answer?.ok) {
    connected = true;
    setStatus("ok", "Connected");
  } else if (answer?.error?.kind === "protocol") {
    connected = false;
    setStatus("warn", "Version mismatch");
  } else {
    connected = false;
    setStatus("offline", "Not running");
  }
}

function pollStatus() {
  clearTimeout(statusTimer);
  if (document.hidden) return;
  void checkApp().then(() => {
    // While the app is reachable its Settings page is the owner; follow it
    // without a reload. A save in flight is left alone until it lands.
    if (connected) void adoptAppRules();
  });
  statusTimer = setTimeout(pollStatus, STATUS_POLL);
}

retry.addEventListener("click", () => {
  setStatus("checking", "Connecting");
  void checkApp();
});

document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    clearTimeout(statusTimer);
  } else {
    pollStatus();
    void adoptAppRules();
  }
});

/* -------------------------------------------------------------- lifecycle */

typeInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    addType();
  } else if (event.key === "Escape") {
    note(typeNote, "");
  }
});

siteInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    addSite();
  } else if (event.key === "Escape") {
    note(siteNote, "");
  }
});

typeInput.addEventListener("input", () => note(typeNote, ""));
siteInput.addEventListener("input", () => note(siteNote, ""));
document.getElementById("type-add").addEventListener("click", addType);
document.getElementById("site-add").addEventListener("click", addSite);

browser.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  // An echo of this page's own write would undo a change made since; the
  // pending save writes the newer state anyway.
  if (saveTimer || saving) return;
  void bdmLoadSettings().then((next) => {
    if (JSON.stringify(next) === JSON.stringify(settings)) return;
    settings = next;
    apply();
  });
});

window.addEventListener("unload", () => {
  clearTimeout(saveTimer);
  clearTimeout(savedTimer);
  clearTimeout(statusTimer);
  clearTimeout(resetTimer);
});

bdmLoadSettings()
  .then((loaded) => {
    settings = loaded;
    bdmApplyPageTheme(settings.theme);
    apply();
  })
  .catch(() => {
    apply();
  })
  .finally(() => {
    void bdmSyncAppTheme(document.documentElement, false);
    pollStatus();
    void adoptAppRules();
  });
