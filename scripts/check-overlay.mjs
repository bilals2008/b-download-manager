// Behaviour checks for the Firefox video overlay.
//
// These run the extension's own content scripts in a fake page (scripts/lib/
// fake-dom.mjs): the registry that finds videos, the page reader that turns a
// player's requests into addresses, and the overlay that draws the button. The
// scenarios are the ones that broke in the browser — a player whose controls
// sit above the picture, a stream that arrives after the pointer, a navigation
// inside the document, fullscreen, and a page that unloads.
//
// Loaded from scripts/check-extension.mjs, so `npm run check:extension` runs
// them with the rest.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";

import { createFakeDom } from "./lib/fake-dom.mjs";

const rootDir = fileURLToPath(new URL("../extension-firefox/", import.meta.url));
const read = (file) => readFileSync(path.join(rootDir, file), "utf8");

/** The order the manifest injects, so the check runs what Firefox runs. */
const CONTENT_SCRIPTS = [
  "lib/media.js",
  "lib/manifest.js",
  "lib/settings.js",
  "lib/icons.js",
  "content/page-media.js",
  "content/video-registry.js",
  "content/overlay-style.js",
  "content/overlay.js",
];

const RANGED_1080 =
  "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-abc&itag=137" +
  "&mime=video%2Fmp4&clen=24117248&range=0-1048575&rn=8&rbuf=0&sig=xyz";
const AUDIO_140 =
  "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-abc&itag=140" +
  "&mime=audio%2Fwebm&clen=1048576&sig=abc";
const MUXED_22 =
  "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-next&itag=22" +
  "&mime=video%2Fmp4&clen=9000000&sig=next";
const MUXED_18 =
  "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-third&itag=18" +
  "&mime=video%2Fmp4&clen=5000000&sig=third";

/** The whole-file address: the range parameters are what the player added. */
const wholeFile = (url) => {
  const parsed = new URL(url);
  for (const name of ["range", "rn", "rbuf"]) parsed.searchParams.delete(name);
  return parsed.href;
};

/**
 * Build a YouTube-shaped player: a video inside a container, with the player's
 * own gradient and controls as siblings above the picture.
 */
function youTubePlayer(dom, options = {}) {
  const player = dom.create("div");
  player.className = "html5-video-player";
  dom.layout(player, options.box || { left: 0, top: 100, width: 1280, height: 720 });
  dom.mount(player);

  const container = dom.create("div");
  container.className = "html5-video-container";
  player.appendChild(container);

  const video = dom.create("video");
  video.className = "video-stream";
  video.currentSrc = "blob:https://www.youtube.com/7f2c1e";
  video.src = "blob:https://www.youtube.com/7f2c1e";
  video.mediaKeys = null;
  video.querySelectorAll = () => [];
  dom.layout(video, options.videoBox || { left: 0, top: 100, width: 1280, height: 720 });
  container.appendChild(video);

  const controls = dom.create("div");
  controls.className = "ytp-chrome-bottom";
  dom.layout(controls, { left: 12, top: 700, width: 1240, height: 48 });
  player.appendChild(controls);

  return { player, container, video, controls };
}

/** The quality list the fake application returns for a media job. */
function mediaView() {
  return {
    job: 1,
    state: "ready",
    error: null,
    notice: null,
    pageUrl: "https://www.youtube.com/watch?v=abc",
    title: "A Test Video",
    duration: null,
    formats: [
      {
        id: "137",
        kind: "video",
        container: "mp4",
        codec: "avc1.640028",
        width: 1920,
        height: 1080,
        fps: 30,
        bandwidth: 4200000,
        size: 100,
        label: "1080p",
      },
      {
        id: "140",
        kind: "audio",
        container: "m4a",
        codec: null,
        width: null,
        height: null,
        fps: null,
        bandwidth: 128000,
        size: 20,
        label: "Audio",
      },
    ],
    pairs: [{ video: "137", audio: "140", label: "1080p" }],
    preferred: "1080p",
    cookiesOffered: false,
    cookieConsent: true,
    downloading: false,
    done: false,
    dest: null,
    stage: null,
    downloaded: 0,
    total: 0,
    segmentsDone: 0,
    segmentsTotal: 0,
  };
}

/** A plain page with one video file, for the non-YouTube cases. */
function filePage(dom, src = "https://cdn.example.org/clip.mp4") {
  const video = dom.create("video");
  video.currentSrc = src;
  video.src = src;
  video.mediaKeys = null;
  video.querySelectorAll = () => [];
  dom.layout(video, { left: 40, top: 120, width: 800, height: 450 });
  dom.mount(video);
  return video;
}

async function startPage(options = {}) {
  const dom = createFakeDom(options);
  Object.assign(dom.storedSettings, options.settings || {});

  const calls = [];
  dom.calls = calls;
  dom.list = () => ({ downloads: [], active: 0 });
  dom.browser.runtime.sendMessage = async (message) => {
    calls.push(message);
    if (options.send) return options.send(message);
    if (message.type === "bdm:media") {
      return { ok: true, result: { job: 1 } };
    }
    if (message.type === "bdm:media-status") {
      return { ok: true, view: mediaView() };
    }
    if (message.type === "bdm:media-start") {
      return { ok: true, result: { download: 1, downloads: [1] } };
    }
    if (message.type === "bdm:media-allow") {
      return { ok: true, view: mediaView() };
    }
    if (message.type === "bdm:send") {
      return { ok: true, result: { mode: "ask" } };
    }
    if (message.type === "bdm:list") return { ok: true, ...dom.list() };
    return { ok: true };
  };

  for (const file of options.seed || []) dom.addResource(file);

  const context = vm.createContext({ ...dom.globals });
  for (const file of CONTENT_SCRIPTS) {
    vm.runInContext(read(file), context, { filename: file });
  }
  await dom.settle();

  const host = () => dom.document.querySelector("[data-bdm-overlay]");
  const shadow = () => host()?.shadowRootClosed || null;
  const visible = () => host()?.getAttribute("data-visible") === "true";
  const open = () => host()?.getAttribute("data-open") === "true";
  const rows = () => shadow()?.querySelectorAll(".row") || [];
  const status = () => shadow()?.querySelector(".status")?.textContent || "";

  return {
    dom,
    context,
    host,
    shadow,
    visible,
    open,
    rows,
    status,
    hover(target, x, y) {
      return dom.fire(target, "pointerover", { clientX: x, clientY: y });
    },
    close() {
      dom.fire(dom.window, "pagehide", { persisted: false });
    },
  };
}

/* --------------------------------------------- the button on a YouTube player */

{
  const page = await startPage({
    url: "https://www.youtube.com/watch?v=abc",
    seed: [RANGED_1080, AUDIO_140],
  });
  const { player, container, video, controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  assert.equal(page.host(), null, "nothing is drawn before the pointer arrives");

  // The pointer is over the player's controls, not over the <video>: that is
  // where a real pointer spends its time on YouTube.
  page.hover(controls, 600, 720);
  await page.dom.settle();

  assert.ok(page.host(), "the button exists after hovering the player");
  assert.equal(page.visible(), true, "the button is visible over the player controls");
  assert.match(
    page.host().style.transform,
    /translate3d\(\d+px, \d+px, 0\)/,
    "the button is placed at the video's corner",
  );

  // Hovering the picture itself must find the same video and draw no second
  // button.
  for (let index = 0; index < 5; index += 1) page.hover(container, 400, 300);
  await page.dom.settle();
  assert.equal(
    page.dom.document.querySelectorAll("[data-bdm-overlay]").length,
    1,
    "only one button exists however often the pointer moves",
  );

  // Open the panel from the button.
  const fab = page.shadow().querySelector(".fab");
  page.dom.fire(fab, "click", { clientX: 0, clientY: 0 });
  await page.dom.settle();

  assert.equal(page.open(), true, "the button opens the panel inline");
  assert.equal(page.rows().length, 1, "the panel lists the parsed quality pairs");
  assert.equal(
    page.rows()[0].querySelector(".name").textContent,
    "1080p",
    "the row names the quality from the app's manifest",
  );
  assert.equal(
    page.rows()[0].querySelector(".size").textContent,
    "120 B",
    "the row carries the size the manifest gave",
  );

  const media = page.dom.calls.filter((message) => message.type === "bdm:media");
  assert.equal(media.length, 2, "the picture and its sound go to the app");
  assert.equal(
    media[0].payload.manifestUrl,
    wholeFile(RANGED_1080),
    "the whole file is sent, not a range",
  );
  assert.equal(media[0].payload.kind, "file", "a YouTube stream is a plain file");
  assert.equal(media[0].payload.presentation, "inline", "the panel stays inline");
  assert.equal(media[0].payload.referer, "https://www.youtube.com/watch?v=abc");
  assert.ok(
    media.some((message) => message.payload.manifestUrl === wholeFile(AUDIO_140)),
    "the sound of the same capture goes with the picture",
  );

  // Pick the quality and hand it to the app.
  const action = page.rows()[0].querySelector(".row-action");
  page.dom.fire(action, "click");
  await page.dom.settle();

  const started = page.dom.calls.find(
    (message) => message.type === "bdm:media-start",
  );
  assert.ok(started, "the quality is started through the app");
  assert.equal(started.job, 1);
  assert.equal(started.payload.video, "137");
  assert.equal(started.payload.audio, "140");
  assert.equal(
    page.status(),
    "Downloading in B Download Manager",
    "the panel says where the download went",
  );

  // Leaving the player hides the button.
  page.dom.fire(page.dom.body, "pointerover", { clientX: 1180, clientY: 20 });
  await new Promise((resolve) => setTimeout(resolve, 520));
  assert.equal(page.visible(), false, "the button leaves with the pointer");
  assert.equal(page.open(), false, "the panel closes with the button");

  page.close();
}

/* ---------------------------------- a stream that arrives after the pointer */

{
  const page = await startPage({ url: "https://www.youtube.com/watch?v=abc" });
  const { controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  page.hover(controls, 600, 720);
  await page.dom.settle();
  assert.equal(page.visible(), false, "a player with no address shows no button");

  // The player asks for its first segment while the pointer waits.
  page.dom.pushResource(RANGED_1080);
  await page.dom.settle();

  assert.equal(page.visible(), true, "the first stream brings the button up");
  assert.equal(page.rows().length, 0, "the panel is closed, so no row is drawn yet");

  page.close();
}

/* ------------------------------------------ a video that the page adds later */

{
  const page = await startPage({ url: "https://example.org/article" });
  assert.equal(page.host(), null, "a page with no video draws nothing");

  const video = filePage(page.dom);
  await page.dom.settle();

  page.hover(video, 200, 200);
  await page.dom.settle();
  assert.equal(page.visible(), true, "a video inserted later gets a button");

  const fab = page.shadow().querySelector(".fab");
  page.dom.fire(fab, "click");
  await page.dom.settle();
  assert.equal(page.rows().length, 1, "the file is offered");
  assert.equal(
    page.rows()[0].querySelector(".name").textContent,
    "1080p",
    "the application's quality list is drawn inline",
  );
  const sentFile = page.dom.calls.find((message) => message.type === "bdm:media");
  assert.ok(sentFile, "the element's own file is handed to the app");
  assert.equal(
    sentFile.payload.manifestUrl,
    "https://cdn.example.org/clip.mp4",
    "the element's own file is offered, not the captured streams",
  );
  assert.equal(sentFile.payload.kind, "file");

  // A player that replaces its video must not leave the button behind.
  video.remove();
  await page.dom.settle();
  assert.equal(page.visible(), false, "removing the video removes the button");

  page.close();
}

/* ------------------------------- a navigation inside the document (YouTube) */

{
  const page = await startPage({
    url: "https://www.youtube.com/watch?v=first",
    seed: [RANGED_1080, AUDIO_140],
  });
  const { video, controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  page.hover(controls, 600, 720);
  const fab = page.shadow().querySelector(".fab");
  page.dom.fire(fab, "click");
  await page.dom.settle();
  assert.equal(page.rows().length, 1, "the first video offers its qualities");

  // The player navigates to the next video without reloading the document, and
  // reuses the same <video> element for it.
  page.dom.globals.location.href = "https://www.youtube.com/watch?v=second";
  page.dom.pushResource(MUXED_22);
  await page.dom.settle();

  assert.equal(page.visible(), true, "the button stays while the next video starts");
  assert.equal(
    page.open(),
    false,
    "the previous video's quality list is closed with the navigation",
  );
  assert.equal(page.rows().length, 0, "the previous video's rows are dropped");

  // The new video's button reads its own capture.
  page.dom.fire(page.shadow().querySelector(".fab"), "click");
  await page.dom.settle();
  const lastMedia = page.dom.calls
    .filter((message) => message.type === "bdm:media")
    .at(-1);
  assert.equal(
    lastMedia.payload.manifestUrl,
    wholeFile(MUXED_22),
    "the new video's stream is the one handed over",
  );
  assert.equal(page.rows().length, 1, "the new quality list is shown");

  // The next navigation happens before the player asks for a stream. The popup
  // asks first, and must not see the video that was left behind either.
  page.dom.globals.location.href = "https://www.youtube.com/watch?v=third";
  const asked = await page.dom.messageListeners.at(-1)({ type: "bdm:media-list" });
  assert.deepEqual(
    Array.from(asked.items, (item) => item.url),
    [],
    "the popup sees the current page only",
  );

  page.hover(video, 400, 300);
  await page.dom.settle();
  assert.equal(page.visible(), false, "a video with no stream yet shows no button");
  assert.equal(page.open(), false, "and no panel is left behind");

  // The first segment of the third video brings the button back without a new
  // hover, because the pointer never left the player.
  page.dom.pushResource(MUXED_18);
  await page.dom.settle();
  assert.equal(page.visible(), true, "the first stream of the new video restores the button");

  const askedAgain = await page.dom.messageListeners.at(-1)({ type: "bdm:media-list" });
  assert.deepEqual(
    Array.from(askedAgain.items, (item) => item.url),
    [wholeFile(MUXED_18)],
    "the popup lists the stream of the video that is playing",
  );

  page.dom.fire(page.shadow().querySelector(".fab"), "click");
  await page.dom.settle();
  const currentMedia = page.dom.calls
    .filter((message) => message.type === "bdm:media")
    .at(-1);
  assert.equal(
    currentMedia.payload.manifestUrl,
    wholeFile(MUXED_18),
    "the panel offers the stream that is playing now",
  );
  assert.equal(page.rows().length, 1, "the quality row of the current stream is drawn");

  page.close();
}

/* ------------------------------------------------------------ unsupported */

{
  const page = await startPage({ url: "https://example.org/protected" });
  const video = filePage(page.dom);
  // An EME element has no address the application may read.
  video.mediaKeys = {};

  page.hover(video, 200, 200);
  await page.dom.settle();
  assert.equal(page.host(), null, "a protected video offers no button");

  page.close();
}

{
  const page = await startPage({ url: "https://example.org/off", settings: { overlay: false } });
  const video = filePage(page.dom);

  page.hover(video, 200, 200);
  await page.dom.settle();
  assert.equal(page.host(), null, "the switch turns the button off");
  assert.ok(
    page.dom.calls.some((message) => message.type === "bdm:media-count" && message.count === 0),
    "a switched-off overlay clears the badge",
  );

  page.close();
}

/* ------------------------------------------------------------- fullscreen */

{
  const page = await startPage({
    url: "https://www.youtube.com/watch?v=abc",
    seed: [RANGED_1080],
  });
  const { player, controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  page.hover(controls, 600, 720);
  assert.equal(page.visible(), true);

  page.dom.layout(player, { left: 0, top: 0, width: 1920, height: 1080 });
  page.dom.setFullscreen(player);
  await page.dom.settle();

  assert.equal(page.host().parentNode, player, "the host is mounted in the fullscreen element");
  assert.equal(page.visible(), true, "the button stays visible in fullscreen");

  page.dom.setFullscreen(null);
  await page.dom.settle();
  assert.equal(
    page.host().parentNode,
    page.dom.document.documentElement,
    "leaving fullscreen returns the host to the document",
  );

  page.close();
}

/* ------------------------------------------------ the application is closed */

{
  const page = await startPage({
    url: "https://www.youtube.com/watch?v=abc",
    seed: [RANGED_1080],
    send: (message) => {
      if (message.type === "bdm:media" || message.type === "bdm:send") {
        throw new TypeError("Failed to fetch");
      }
      return { ok: true };
    },
  });
  const { controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  page.hover(controls, 600, 720);
  const fab = page.shadow().querySelector(".fab");
  page.dom.fire(fab, "click");
  await page.dom.settle();

  assert.equal(page.status(), "B Download Manager is not running", "a closed app is named");
  assert.equal(page.rows().length, 0, "no quality list is invented when the app is closed");

  page.close();
}

/* ------------------------------------------------------------- taking leave */

{
  const page = await startPage({
    url: "https://www.youtube.com/watch?v=abc",
    seed: [RANGED_1080],
  });
  const { controls } = youTubePlayer(page.dom);
  await page.dom.settle();
  page.hover(controls, 600, 720);
  assert.equal(page.visible(), true);

  page.close();
  await page.dom.settle();

  assert.equal(
    page.dom.document.querySelectorAll("[data-bdm-overlay]").length,
    0,
    "the button is removed when the page goes away",
  );
  assert.equal(page.dom.observers.size, 0, "the video observer is disconnected");
  assert.equal(page.dom.settingsListeners.length, 0, "the settings listener is removed");
  assert.equal(page.dom.resizeObservers.size, 0, "the resize observer is disconnected");
}

/* ------------------------------------- the quality panel owns media progress */

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

{
  const page = await startPage({
    url: "https://www.youtube.com/watch?v=abc",
    seed: [RANGED_1080, AUDIO_140],
  });
  const { controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  page.hover(controls, 600, 720);
  page.dom.fire(page.shadow().querySelector(".fab"), "click");
  await page.dom.settle();
  page.dom.fire(page.rows()[0].querySelector(".row-action"), "click");
  await page.dom.settle();

  const fab = page.shadow().querySelector(".fab");
  assert.equal(
    fab.getAttribute("data-progress"),
    "false",
    "the row in the home list carries progress, not the button ring",
  );
  assert.ok(
    page.dom.calls.some((message) => message.type === "bdm:media-start"),
    "the chosen quality was sent to the app",
  );
  await wait(1100);
  assert.equal(page.open(), false, "the panel closes after the download starts");
  const before = page.dom.calls.filter((message) => message.type === "bdm:list").length;
  await wait(900);
  const after = page.dom.calls.filter((message) => message.type === "bdm:list").length;
  assert.equal(after, before, "no download polling is started by the overlay");

  page.close();
}

console.log("overlay: all checks pass");
