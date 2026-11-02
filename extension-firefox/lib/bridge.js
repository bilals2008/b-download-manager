/*
  The loopback bridge, in one place.

  The application listens on 127.0.0.1:27500 and answers JSON. The contract is
  versioned: the ping names the app and the API, so a stale process on the port
  is told apart from a missing application.

  One header says who is asking: the browser extension. A web page cannot put
  it on a cross-origin request without a preflight, and the preflight is
  refused unless the origin is an extension's. Nothing is pasted and there is no
  pairing step: the extension and the app find each other as soon as both are
  installed.

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
          kind: bdmFailureKind(response.status),
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

/** What one status code means for what the user has to do next. */
function bdmFailureKind(status) {
  if (status === 403) return "forbidden";
  return "refused";
}

/**
 * The handshake. Confirms that the app, not another process, holds the port.
 *
 * The answer carries `connected`, so this one call says both that the
 * application is there and that it heard the extension's own header.
 */
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

/**
 * Write capture rules to the application.
 *
 * The application's Settings page owns these while it runs, so a change made
 * in this extension's options page has to go there too; otherwise the two
 * pages would disagree and the app's copy would keep winning.
 */
async function bdmBridgeCaptureSave(policy) {
  return bdmBridgeRequest(
    "POST",
    "/v1/capture",
    {
      capture: policy?.capture === true,
      ask: policy?.ask === true,
      types: Array.isArray(policy?.types) ? policy.types : [],
      skipSites: Array.isArray(policy?.skipSites) ? policy.skipSites : [],
    },
    4000,
  );
}

/**
 * The appearance the application gives the floating video button.
 *
 * The application's Settings page owns this; the overlay only draws what it
 * reads. A failure is a failure — the caller keeps the last appearance it saw
 * and says so, rather than reporting the app's choice as the default one.
 */
async function bdmBridgeVideoButton() {
  return bdmBridgeRequest("GET", "/v1/video-button-settings", undefined, 2500);
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

/** Register a stream manifest; the answer names the job the panel watches. */
async function bdmBridgeMedia(payload) {
  return bdmBridgeRequest(
    "POST",
    "/v1/media",
    {
      pageUrl: String(payload?.pageUrl || ""),
      manifestUrl: String(payload?.manifestUrl || ""),
      kind:
        payload?.kind === "hls" ||
        payload?.kind === "dash" ||
        payload?.kind === "file"
          ? payload.kind
          : "auto",
      title: String(payload?.title || ""),
      referer: String(payload?.referer || ""),
      headers:
        payload?.headers && typeof payload.headers === "object"
          ? payload.headers
          : undefined,
      presentation: payload?.presentation === "inline" ? "inline" : "window",
    },
    8000,
  );
}

/**
 * What the link window may choose from: the folders, the queue and the
 * default connection count.
 *
 * The folders are the places the application already uses, not a path the
 * extension typed: the bridge refuses one it did not offer.
 */
async function bdmBridgeBatchOptions() {
  return bdmBridgeRequest("GET", "/v1/batch-options", undefined, 2500);
}

/**
 * Hand over one chunk of ticked links.
 *
 * The app answers one line per link: started, queued, duplicate, invalid or
 * refused, with the sentence for the ones that did not start. A large
 * selection is sent in chunks, so this request stays small.
 */
async function bdmBridgeBatch(payload) {
  const items = Array.isArray(payload?.items) ? payload.items : [];
  return bdmBridgeRequest(
    "POST",
    "/v1/downloads/batch",
    {
      items: items.map((item) => ({
        url: String(item?.url || ""),
        name: String(item?.name || ""),
        category: String(item?.category || ""),
      })),
      folder: String(payload?.folder || ""),
      // Zero means "whatever the app offered", which is `null` here.
      connections: Number(payload?.connections) > 0 ? Number(payload.connections) : null,
      queue: payload?.queue === "main" ? "main" : "now",
      referer: String(payload?.referer || ""),
    },
    15000,
  );
}

/** What the application found in a media job. */
async function bdmBridgeMediaStatus(job) {
  return bdmBridgeRequest("GET", `/v1/media/${job}`, undefined, 4000);
}

/** Start one quality of a media job. */
async function bdmBridgeMediaStart(job, payload) {
  return bdmBridgeRequest(
    "POST",
    `/v1/media/${job}/download`,
    {
      video: String(payload?.video || ""),
      audio: payload?.audio ? String(payload.audio) : null,
      name: String(payload?.name || ""),
      dest: String(payload?.dest || ""),
    },
    8000,
  );
}

/** Allow cookies for media requests, once, and read the job again. */
async function bdmBridgeMediaAllow(job) {
  return bdmBridgeRequest("POST", `/v1/media/${job}/allow-cookies`, {}, 8000);
}

/** Stop a media job. */
async function bdmBridgeMediaCancel(job) {
  return bdmBridgeRequest("POST", `/v1/media/${job}/cancel`, {}, 4000);
}

/** Pause, resume, stop or show one download in the app. */
async function bdmBridgeControl(id, action) {
  return bdmBridgeRequest("POST", `/v1/downloads/${id}/${action}`, {}, 4000);
}

/** Bring the application window to the front. */
async function bdmBridgeFocus() {
  return bdmBridgeRequest("POST", "/v1/focus", {}, 2500);
}
