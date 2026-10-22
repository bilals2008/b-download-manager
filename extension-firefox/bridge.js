/*
  The loopback bridge, in one place. The application listens on
  127.0.0.1:27500 and opens its add dialog for whatever link it is given; the
  contract is in `src-tauri/src/bridge.rs`.

  Loaded by the background page and by the popup, so the address, the ping and
  the hand-over are written once.
*/

const APP = "http://127.0.0.1:27500";
const LINK = /^(https?|ftp):/;

/*
  The bridge answers "ok" to its ping and nothing else does, so a stale
  process holding the port is not mistaken for the application.
*/
async function appIsRunning() {
  try {
    const answer = await fetch(`${APP}/ping`, { signal: AbortSignal.timeout(800) });
    return answer.ok && (await answer.text()) === "ok";
  } catch {
    return false;
  }
}

async function handOver(url, name, referer) {
  const query = new URLSearchParams({ url, name: name || "", referer: referer || "" });
  try {
    const answer = await fetch(`${APP}/download?${query}`, {
      signal: AbortSignal.timeout(3000),
    });
    return answer.ok;
  } catch {
    return false;
  }
}

/** The last segment of a file path or of an address. */
function baseName(path) {
  const last = (path || "").split(/[\\/]/).filter(Boolean).pop();
  if (!last) return "";
  try {
    return decodeURIComponent(last);
  } catch {
    return last;
  }
}

function nameOfLink(url) {
  try {
    return baseName(new URL(url).pathname);
  } catch {
    return "";
  }
}
