/* The popup of the Chrome build: one switch and the state of the application.
   There is no code to paste; the extension connects on its own. */

const takeover = document.getElementById("takeover");
const state = document.getElementById("state");
const note = document.getElementById("note");

async function refresh() {
  const settings = await chrome.storage.local.get({ takeover: true });
  takeover.checked = settings.takeover;

  try {
    const answer = await fetch("http://127.0.0.1:27500/v1/ping", {
      cache: "no-store",
      headers: { "X-BDM-Client": "browser-extension" },
    });
    const payload = await answer.json().catch(() => null);
    if (!payload?.ok || payload.data?.app !== "b-download-manager") {
      state.textContent = "Another program is using the bridge port.";
      note.textContent = "";
      return;
    }
    state.textContent = "B Download Manager is running.";
    note.textContent = "Downloads from this browser reach the app.";
  } catch {
    state.textContent = "B Download Manager is not running.";
    note.textContent = "";
  }
}

takeover.addEventListener("change", () => {
  chrome.storage.local.set({ takeover: takeover.checked });
});

refresh();
