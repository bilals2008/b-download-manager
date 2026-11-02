/* The popup of the Chrome build: one switch, the pairing code, and the state of
   the application. */

const takeover = document.getElementById("takeover");
const state = document.getElementById("state");
const token = document.getElementById("token");
const pair = document.getElementById("pair");

const CODE_SHAPE = /^[0-9a-f]{32,128}$/;

async function refresh() {
  const settings = await chrome.storage.local.get({ takeover: true, token: "" });
  takeover.checked = settings.takeover;
  // The code is shown back so the reader can see what is stored; it is a
  // pairing secret for this machine only.
  token.value = settings.token || "";

  try {
    const answer = await fetch("http://127.0.0.1:27500/v1/ping", {
      cache: "no-store",
      headers: {
        "X-BDM-Client": "browser-extension",
        ...(settings.token ? { Authorization: `Bearer ${settings.token}` } : {}),
      },
    });
    const payload = await answer.json().catch(() => null);
    if (!payload?.ok || payload.data?.app !== "b-download-manager") {
      state.textContent = "Another program is using the bridge port.";
      pair.textContent = "";
      return;
    }
    state.textContent = "B Download Manager is running.";
    pair.textContent = payload.data.paired
      ? "Paired. Downloads from this browser reach the app."
      : "Not paired. Paste the code from the app's Settings, Browser integration.";
  } catch {
    state.textContent = "B Download Manager is not running.";
    pair.textContent = "";
  }
}

takeover.addEventListener("change", () => {
  chrome.storage.local.set({ takeover: takeover.checked });
});

token.addEventListener("change", async () => {
  const clean = token.value.trim().toLowerCase();
  if (!clean) {
    await chrome.storage.local.remove("token");
    pair.textContent = "Code removed. Paste one to pair again.";
    return;
  }
  if (!CODE_SHAPE.test(clean)) {
    pair.textContent = "That is not a code: 32 to 128 characters, 0-9 and a-f only.";
    return;
  }
  await chrome.storage.local.set({ token: clean });
  token.value = clean;
  void refresh();
});

refresh();
