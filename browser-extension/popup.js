/* The popup of the Chrome build: one switch and the state of the application. */

const takeover = document.getElementById("takeover");
const state = document.getElementById("state");

async function refresh() {
  const settings = await chrome.storage.local.get({ takeover: true });
  takeover.checked = settings.takeover;

  try {
    const answer = await fetch("http://127.0.0.1:27500/v1/ping", {
      cache: "no-store",
      headers: { "X-BDM-Client": "browser-extension" },
    });
    const payload = await answer.json().catch(() => null);
    state.textContent =
      payload?.ok && payload.data?.app === "b-download-manager"
        ? "Connected to B Download Manager."
        : "Another program is using the bridge port.";
  } catch {
    state.textContent = "B Download Manager is not running.";
  }
}

takeover.addEventListener("change", () => {
  chrome.storage.local.set({ takeover: takeover.checked });
});

refresh();
