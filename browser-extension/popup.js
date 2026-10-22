/* The popup of the extension: one switch and the state of the application. */

const takeover = document.getElementById("takeover");
const state = document.getElementById("state");

async function refresh() {
  const settings = await chrome.storage.local.get({ takeover: true });
  takeover.checked = settings.takeover;

  try {
    const answer = await fetch("http://127.0.0.1:27500/ping");
    state.textContent = answer.ok
      ? "The application is running."
      : "The application is not answering.";
  } catch {
    state.textContent = "The application is not running.";
  }
}

takeover.addEventListener("change", () => {
  chrome.storage.local.set({ takeover: takeover.checked });
});

refresh();
