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
  // A timeout of 0 means "wait as long as it takes", for the mux download.
  const timer = timeout > 0 ? setTimeout(() => controller.abort(), timeout) : 0;

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
    if (timer) clearTimeout(timer);
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

/** Pause, resume, stop or show one download in the app. */
async function bdmBridgeControl(id, action) {
  return bdmBridgeRequest("POST", `/v1/downloads/${id}/${action}`, {}, 4000);
}

/** Which bundled media tools the app can run. */
async function bdmBridgeMediaTools() {
  return bdmBridgeRequest("GET", "/v1/media/tools", undefined, 4000);
}

/**
 * The formats of a video page, as the extractor reports them.
 *
 * The address is the page the user is watching; the app runs yt-dlp against it
 * and answers with the complete list (resolution, fps, codec, container, size,
 * audio/video) and a fresh direct address for each. A page the extractor
 * cannot read is answered with an error rather than a guess.
 */
async function bdmBridgeExtractFormats(url) {
  const encoded = encodeURIComponent(url);
  return bdmBridgeRequest("GET", `/v1/extract-formats?url=${encoded}`, undefined, 30000);
}

/**
 * Download and mux a video and an audio stream the user picked.
 *
 * The streams are the fresh addresses the extractor returned. The mux runs to
 * completion before the app answers, so a successful answer means the file is
 * already on disk and has been probed as playable.
 */
async function bdmBridgeMux(payload) {
  return bdmBridgeRequest(
    "POST",
    "/v1/downloads/mux",
    {
      videoUrl: payload.videoUrl,
      audioUrl: payload.audioUrl || "",
      container: payload.container || "mp4",
      name: payload.name || "",
    },
    0,
  );
}

/** Bring the application window to the front. */
async function bdmBridgeFocus() {
  return bdmBridgeRequest("POST", "/v1/focus", {}, 2500);
}
