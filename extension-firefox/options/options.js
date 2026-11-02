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
const tokenInput = document.getElementById("token-input");
const tokenSave = document.getElementById("token-save");
const tokenClear = document.getElementById("token-clear");
const tokenNote = document.getElementById("token-note");

const SAVE_DELAY = 300;
const STATUS_POLL = 5000;
/** The key the background page reads the pairing code from. */
const TOKEN_KEY = "token";
/** The shape the application writes: hex, at least 32 characters. A typo is
 *  caught here rather than turning every hand-over into a refusal. */
const TOKEN_SHAPE = /^[0-9a-f]{32,128}$/;

let settings = bdmNormalizeSettings({});
let saveTimer = 0;
let saving = 0;
let savedTimer = 0;
let statusTimer = 0;
let resetTimer = 0;
let resetArmed = false;
let checking = false;
/** What the application last said about this extension, and the stored code. */
let paired = null;
let hasCode = false;
/** A sentence the reader needs until the next thing they do. */
let pairingHint = "";

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
 * One line about the pairing, in the reader's terms.
 *
 * The application answers whether the code this extension sends is its own, so
 * nothing here guesses from a failed request: "Not running", "Paste the code"
 * and "The app does not know this code" are three different fixes.
 */
function renderPairing() {
  if (pairingHint) {
    note(tokenNote, pairingHint);
    return;
  }
  if (paired === null) {
    note(tokenNote, hasCode ? "Checking with the app…" : "No code saved yet.");
    return;
  }
  if (paired === false && hasCode === false) {
    note(tokenNote, "Paste the code from the app and save it.");
    return;
  }
  if (paired) {
    note(tokenNote, "Paired. Downloads from this extension reach the app.");
    return;
  }
  note(tokenNote, "The app is running but does not know this code. Make a new one there.");
}

async function checkApp() {
  if (checking) return;
  checking = true;
  const answer = await browser.runtime
    .sendMessage({ type: "bdm:status" })
    .catch(() => null);
  checking = false;

  if (answer?.ok) {
    paired = answer.paired === true;
    hasCode = answer.hasCode === true;
    // A reachable app that does not know this extension is not "Connected": the
    // hand-overs will not work, and the page has to say so.
    setStatus(paired ? "ok" : "warn", paired ? "Connected" : "Not paired");
  } else {
    paired = null;
    hasCode = answer?.hasCode === true;
    setStatus("offline", "Not running");
  }
  renderPairing();
}

/* ---------------------------------------------------------------- pairing */

async function saveCode() {
  const clean = tokenInput.value.trim().toLowerCase();
  if (!clean) {
    pairingHint = "Nothing to save — paste the code first.";
    renderPairing();
    return;
  }
  if (!TOKEN_SHAPE.test(clean)) {
    pairingHint = "That is not a code: 32 to 128 characters, 0-9 and a-f only.";
    renderPairing();
    tokenInput.focus();
    return;
  }

  pairingHint = "";
  try {
    await browser.storage.local.set({ [TOKEN_KEY]: clean });
  } catch {
    pairingHint = "The code could not be stored. Try again.";
    renderPairing();
    return;
  }

  hasCode = true;
  tokenInput.value = "";
  // The background page takes the code up from storage, so asking it now asks
  // the app with it — and the answer is the app's own, not a guess.
  const answer = await browser.runtime.sendMessage({ type: "bdm:pairing-check" }).catch(() => null);
  if (answer?.ok) {
    paired = answer.paired === true;
    setStatus(paired ? "ok" : "warn", paired ? "Connected" : "Not paired");
    pairingHint = paired
      ? "Saved. This extension is paired."
      : "Saved, but the app says it is not its current code.";
  } else if (answer?.error?.kind === "offline") {
    paired = null;
    pairingHint = "Saved. The app is not running, so it could not confirm it yet.";
  } else {
    paired = null;
    pairingHint = "Saved. The app could not be asked to confirm it.";
  }
  renderPairing();
}

async function clearCode() {
  await browser.storage.local.remove(TOKEN_KEY).catch(() => {});
  hasCode = false;
  paired = null;
  pairingHint = "Code removed. The app will not take downloads from here until you pair again.";
  renderPairing();
  tokenInput.focus();
}

function pollStatus() {
  clearTimeout(statusTimer);
  if (document.hidden) return;
  void checkApp();
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

tokenInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    void saveCode();
  } else if (event.key === "Escape") {
    tokenInput.value = "";
    pairingHint = "";
    renderPairing();
  }
});

// A code being typed is a question being answered: the standing note comes back
// once the field is empty again.
tokenInput.addEventListener("input", () => {
  pairingHint = "";
  renderPairing();
});
tokenSave.addEventListener("click", () => void saveCode());
tokenClear.addEventListener("click", () => void clearCode());

/** Whether a code is stored, without putting it on the page. */
async function readStoredCode() {
  try {
    const stored = await browser.storage.local.get(TOKEN_KEY);
    hasCode = Boolean(stored?.[TOKEN_KEY]);
  } catch {
    hasCode = false;
  }
}

browser.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (Object.prototype.hasOwnProperty.call(changes, TOKEN_KEY)) {
    hasCode = Boolean(changes[TOKEN_KEY]?.newValue);
    paired = null;
    pairingHint = "";
    renderPairing();
    void checkApp();
  }
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
    void readStoredCode().then(renderPairing);
    pollStatus();
  });
