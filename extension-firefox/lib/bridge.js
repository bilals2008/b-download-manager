/*
  The loopback bridge, in one place.

  The application listens on 127.0.0.1:27500 and answers JSON. The contract is
  versioned: the ping names the app and the API, so a stale process on the port
  is told apart from a missing application. Every request carries the client
  header, which a web page cannot add without a preflight the bridge refuses.

  Loaded by the background page, which is the only context that talks to the
  bridge. The popup and the content script ask the background for what they
  need, so the error handling lives once.
*/

const BDM_BRIDGE = Object.freeze({
  base: "http://127.0.0.1:27500",
  api: 1,
  app: "b-download-manager",
  client: "browser-extension",
});

/** A bridge failure with a kind the UI can turn into one short line. */
class BdmBridgeError extends Error {
  constructor(message, options = {}) {
    super(message);
    this.name = "BdmBridgeError";
    this.kind = options.kind || "http";
    this.status = options.status || 0;
    this.code = options.code || "";
  }
}

/** One request. `body` is a JSON value, or undefined for a GET. */
async function bdmBridgeRequest(method, path, body, timeout = 4000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);

  try {
    const response = await fetch(`${BDM_BRIDGE.base}${path}`, {
      method,
      cache: "no-store",
      credentials: "omit",
      headers: {
        "X-BDM-Client": BDM_BRIDGE.client,
        ...(body === undefined ? {} : { "Content-Type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
    });

    const text = await response.text();
    let payload;
    try {
      payload = JSON.parse(text);
    } catch {
      throw new BdmBridgeError("The app answered with something that is not JSON.", {
        kind: "protocol",
        status: response.status,
      });
    }

    if (!payload || typeof payload !== "object") {
      throw new BdmBridgeError("The app answered with an empty answer.", {
        kind: "protocol",
        status: response.status,
      });
    }

    if (!payload.ok) {
      throw new BdmBridgeError(
        payload.error?.message || `The app refused the request (${response.status}).`,
        {
          kind: "refused",
          status: response.status,
          code: payload.error?.code || "",
        },
      );
    }

    return payload.data;
  } catch (error) {
    if (error instanceof BdmBridgeError) throw error;
    if (error?.name === "AbortError") {
      throw new BdmBridgeError("The app did not answer in time.", { kind: "timeout" });
    }
    throw new BdmBridgeError("The app is not running.", { kind: "offline" });
  } finally {
    clearTimeout(timer);
  }
}

/** The handshake. Confirms that the app, not another process, holds the port. */
async function bdmBridgePing() {
  const data = await bdmBridgeRequest("GET", "/v1/ping", undefined, 1200);
  if (data?.app !== BDM_BRIDGE.app) {
    throw new BdmBridgeError("Another program is using the bridge port.", {
      kind: "protocol",
    });
  }
  if (data.api !== BDM_BRIDGE.api) {
    throw new BdmBridgeError("The extension and the app expect different bridge versions.", {
      kind: "protocol",
    });
  }
  return data;
}

/** The downloads in the app, and how many are unfinished. */
async function bdmBridgeList() {
  return bdmBridgeRequest("GET", "/v1/downloads", undefined, 2500);
}

/**
 * The capture rules the application owns while it is running.
 *
 * The extension keeps its own rules for when the application is closed; this
 * answer is the newer one whenever the bridge can be reached.
 */
async function bdmBridgeCapturePolicy() {
  return bdmBridgeRequest("GET", "/v1/capture", undefined, 1500);
}

/** Start a download in the app. `mode` is "direct" or "ask". */
async function bdmBridgeSend(payload) {
  return bdmBridgeRequest(
    "POST",
    "/v1/downloads",
    {
      url: payload.url,
      name: payload.name || "",
      referer: payload.referer || "",
      mode: payload.mode === "ask" ? "ask" : "direct",
    },
    6000,
  );
}

/** Pause, resume, stop or show one download in the app. */
async function bdmBridgeControl(id, action) {
  return bdmBridgeRequest("POST", `/v1/downloads/${id}/${action}`, {}, 4000);
}

/** Bring the application window to the front. */
async function bdmBridgeFocus() {
  return bdmBridgeRequest("POST", "/v1/focus", {}, 2500);
}
