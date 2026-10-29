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

  assert.equal(page.open(), true, "the button opens the panel");
  assert.equal(page.rows().length, 2, "the panel lists both streams of the video");
  assert.equal(
    page.rows()[0].querySelector(".tag").textContent,
    "MP4",
    "the first row is the picture stream",
  );
  assert.match(
    page.rows()[0].querySelector(".name").textContent,
    /1080p video only/,
    "the row names the quality",
  );
  assert.equal(page.rows()[1].querySelector(".tag").textContent, "AUDIO");

  // Hand the first stream over.
  const action = page.rows()[0].querySelector(".row-action");
  page.dom.fire(action, "click");
  await page.dom.settle();

  const sent = page.dom.calls.find((message) => message.type === "bdm:send");
  assert.ok(sent, "a row click hands the address to the background");
  assert.equal(sent.mode, "ask", "a media hand-over opens the app's dialog");
  assert.equal(sent.payload.url, wholeFile(RANGED_1080), "the whole file is sent, not a range");
  assert.match(sent.payload.name, /1080p/, "the file name carries the quality");
  assert.equal(sent.payload.referer, "https://www.youtube.com/watch?v=abc");
  assert.equal(page.status(), "Confirm it in the app", "the panel says what happens next");
  assert.equal(
    page.rows()[0].querySelector(".row-action").getAttribute("aria-label"),
    "Sent to B Download Manager",
    "the row shows it was handed over",
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
  assert.equal(page.rows()[0].querySelector(".tag").textContent, "MP4");
  assert.equal(
    page.rows()[0].dataset.url,
    "https://cdn.example.org/clip.mp4",
    "the element's own file is offered, not the captured streams",
  );

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
  assert.equal(page.rows().length, 2, "the first video offers its streams");

  // The player navigates to the next video without reloading the document, and
  // reuses the same <video> element for it.
  page.dom.globals.location.href = "https://www.youtube.com/watch?v=second";
  page.dom.pushResource(MUXED_22);
  await page.dom.settle();

  assert.equal(page.visible(), true, "the button stays while the next video starts");
  const urls = page.rows().map((row) => row.dataset.url);
  assert.equal(urls.length, 1, "only the streams of the new video are offered");
  assert.equal(urls[0], wholeFile(MUXED_22), "the previous video's stream is dropped");
  assert.equal(
    page.rows()[0].querySelector(".name").textContent.includes("audio"),
    false,
    "the new stream is named by its own itag",
  );

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
  assert.deepEqual(
    page.rows().map((row) => row.dataset.url),
    [wholeFile(MUXED_18)],
    "the panel offers the stream that is playing now",
  );

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
      if (message.type === "bdm:send") throw new TypeError("Failed to fetch");
      return { ok: true };
    },
  });
  const { controls } = youTubePlayer(page.dom);
  await page.dom.settle();

  page.hover(controls, 600, 720);
  const fab = page.shadow().querySelector(".fab");
  page.dom.fire(fab, "click");
  const action = page.rows()[0].querySelector(".row-action");
  page.dom.fire(action, "click");
  await page.dom.settle();

  assert.equal(page.status(), "B Download Manager is not running", "a closed app is named");
  assert.equal(action.disabled, false, "the row can be tried again");
  assert.equal(
    action.getAttribute("aria-label").startsWith("Download"),
    true,
    "no success is reported for a closed app",
  );

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

/* ------------------------------------- progress on the button, panel closed */

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
  const arc = page.shadow().querySelector(".ring-arc");
  assert.equal(fab.getAttribute("data-progress"), "true", "the button follows the hand-over");
  assert.equal(fab.getAttribute("data-tone"), "waiting", "the add dialog is not confirmed yet");
  assert.equal(
    arc.style.strokeDashoffset,
    "",
    "the waiting ring is drawn by the stylesheet, not by a percent",
  );

  // The user confirms the dialog and the application reports the download.
  page.dom.list = () => ({
    downloads: [
      { id: 7, url: wholeFile(RANGED_1080), status: "running", downloaded: 25, total: 100 },
    ],
    active: 1,
  });
  await wait(1000);

  assert.equal(fab.getAttribute("data-tone"), "running", "the button shows the running download");
  assert.equal(arc.style.strokeDashoffset, "75", "the ring shows a quarter of the file");
  assert.match(fab.getAttribute("aria-label"), /Downloading 25%/, "the button names the progress");

  // The panel can be closed; the button keeps the download.
  page.dom.fire(page.shadow().querySelector(".close"), "click");
  await page.dom.settle();
  assert.equal(page.open(), false, "the panel is closed");
  assert.equal(fab.getAttribute("data-progress"), "true", "progress does not need the panel");

  page.dom.list = () => ({
    downloads: [
      { id: 7, url: wholeFile(RANGED_1080), status: "completed", downloaded: 100, total: 100 },
    ],
    active: 0,
  });
  await wait(1200);

  assert.equal(fab.getAttribute("data-tone"), "success", "a finished download is reported");
  assert.equal(arc.style.strokeDashoffset, "0", "the ring closes when the file is saved");

  // The same button on another video must not claim that progress.
  const other = filePage(page.dom);
  page.hover(other, 200, 200);
  await page.dom.settle();
  assert.equal(
    fab.getAttribute("data-progress"),
    "false",
    "the ring belongs to the video the download came from",
  );

  // Closing the page stops the polling instead of leaving a timer behind.
  page.close();
  await wait(300);
  const before = page.dom.calls.filter((message) => message.type === "bdm:list").length;
  await wait(1300);
  const after = page.dom.calls.filter((message) => message.type === "bdm:list").length;
  assert.equal(after, before, "an unloaded page stops asking the application");
}

console.log("overlay: all checks pass");
