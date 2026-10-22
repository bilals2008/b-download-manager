/* The settings page. Everything it writes is read by settings.js, which the
   background page uses for every download. */

const form = document.getElementById("form");
const takeover = document.getElementById("takeover");
const panel = document.getElementById("panel");
const saved = document.getElementById("saved");
const resetButton = document.getElementById("reset");

let dirty = false;
/* Set just before one of our own writes, so the storage listener does not
   mistake it for another tab and reload the page under the user. */
let justSaved = false;

function note(message) {
  saved.textContent = message;
}

const typesEditor = createListEditor(document.getElementById("types-editor"), {
  clean: cleanType,
  onChange: () => {
    dirty = true;
  },
  onMove: (value, index, total) => note(`${value} moved to position ${index + 1} of ${total}`),
});

const sitesEditor = createListEditor(document.getElementById("sites-editor"), {
  clean: cleanSite,
  onChange: () => {
    dirty = true;
  },
  onMove: (value, index, total) => note(`${value} moved to position ${index + 1} of ${total}`),
});

function apply(settings) {
  takeover.checked = settings.takeover;
  panel.checked = settings.panel;
  typesEditor.set(settings.types);
  sitesEditor.set(settings.skipSites);
  dirty = false;
}

form.addEventListener("input", () => {
  dirty = true;
  if (armed) disarm();
});

loadSettings().then(apply);

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (armed) disarm();
  justSaved = true;
  const clean = await saveSettings({
    takeover: takeover.checked,
    panel: panel.checked,
    types: typesEditor.values(),
    skipSites: sitesEditor.values(),
  });
  apply(clean);
  note("Saved");
});

/* Another tab (or the popup) changed a setting. */
browser.storage.onChanged.addListener(async (_changes, area) => {
  if (area !== "local") return;
  if (justSaved) {
    justSaved = false;
    return;
  }

  const settings = await loadSettings();
  if (dirty) {
    note("Changed in another tab. Save or reload to keep your edits.");
    return;
  }

  apply(settings);
  note("Reloaded from another tab.");
});

let armed = false;
let armTimer = 0;

function disarm() {
  armed = false;
  clearTimeout(armTimer);
  resetButton.textContent = "Reset";
  resetButton.classList.remove("confirming");
}

resetButton.addEventListener("click", async () => {
  if (!armed) {
    armed = true;
    resetButton.textContent = "Reset all?";
    resetButton.classList.add("confirming");
    armTimer = setTimeout(disarm, 4000);
    return;
  }

  disarm();
  justSaved = true;
  apply(await saveSettings(DEFAULT_SETTINGS));
  note("Settings reset to defaults.");
});

document.addEventListener("keydown", (event) => {
  if (armed && event.key === "Escape") disarm();
});
