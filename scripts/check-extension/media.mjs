// lib/media.js and content/page-media.js: the media helpers and the page list.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read } from "./common.mjs";

const mediaContext = vm.createContext({ URL, console });
vm.runInContext(read("lib/manifest.js"), mediaContext, { filename: "manifest.js" });
vm.runInContext(read("lib/media.js"), mediaContext, { filename: "media.js" });
const {
  bdmIsDownloadableSource,
  bdmIsMediaLink,
  bdmIsStreamUrl,
  bdmIsCapturedMedia,
  bdmFullMediaUrl,
  bdmMediaKey,
  bdmItagOf,
  bdmMediaKind,
  bdmMediaSize,
  bdmMediaLabel,
  bdmMediaName,
  bdmMediaExtension,
  bdmMediaFileName,
  bdmNameFromUrl,
  bdmSourceLabel,
  bdmPercentOf,
  bdmFormatSize,
  bdmFileNameOfPath,
} = mediaContext;

assert.ok(bdmIsDownloadableSource("https://example.org/a.mp4"));
assert.ok(!bdmIsDownloadableSource("blob:https://example.org/abc"));
assert.ok(!bdmIsDownloadableSource("https://example.org/live.m3u8"));
assert.ok(!bdmIsDownloadableSource("https://example.org/live.mpd"));
assert.ok(!bdmIsDownloadableSource("javascript:alert(1)"));
assert.ok(bdmIsStreamUrl("https://example.org/live.m3u8?token=1"));
assert.ok(bdmIsMediaLink("https://example.org/a.mp4?x=1"));
assert.ok(!bdmIsMediaLink("https://example.org/page"));
assert.equal(bdmNameFromUrl("https://example.org/My%20Film.mp4?sig=1"), "My Film.mp4");
assert.equal(bdmSourceLabel("https://example.org/a.webm?x=1"), "WEBM");
assert.equal(bdmSourceLabel("https://example.org/videoplayback?id=1"), "FILE");
assert.equal(bdmPercentOf({ total: 200, downloaded: 50, status: "running" }), 25);
assert.equal(bdmPercentOf({ total: 0, downloaded: 50, status: "running" }), 0);
assert.equal(bdmPercentOf({ total: 200, downloaded: 200, status: "completed" }), 100);
assert.equal(bdmFormatSize(1536), "1.50 KB");
assert.equal(bdmFileNameOfPath("C:\\Users\\me\\Downloads\\a.mp4"), "a.mp4");
assert.equal(bdmFileNameOfPath(""), "download");

/* A player's signed range request becomes one whole-file address per itag. */
const ranged =
  "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-abc&itag=137" +
  "&mime=video%2Fmp4&clen=24117248&range=0-1048575&rn=8&rbuf=0&sig=xyz";
const audio = "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-abc&itag=140&mime=audio%2Fwebm";

assert.ok(bdmIsCapturedMedia(ranged), "a player request is captured");
assert.ok(bdmIsCapturedMedia("https://example.org/a.mp4?x=1"), "a fetched file is captured");
assert.ok(!bdmIsCapturedMedia("https://example.org/page"));
assert.ok(!bdmIsCapturedMedia("https://example.org/live.m3u8"), "a manifest is not a file");

assert.equal(bdmItagOf(ranged), "137");
assert.equal(bdmFullMediaUrl(ranged), bdmFullMediaUrl(ranged), "the whole-file address is stable");
const full = bdmFullMediaUrl(ranged);
assert.ok(!/[?&](range|rn|rbuf)=/.test(full), "range parameters are stripped");
assert.ok(full.includes("sig=xyz"), "the signature is kept");
assert.equal(bdmMediaKey(full), bdmMediaKey(ranged), "two ranges share one key");
assert.notEqual(
  bdmMediaKey(ranged),
  bdmMediaKey(ranged.replace("itag=137", "itag=136")),
  "a different itag is a different stream",
);

assert.equal(bdmMediaKind(ranged), "video");
assert.equal(bdmMediaKind(audio), "audio");
assert.equal(bdmMediaKind(ranged.replace("itag=137", "itag=18")), "muxed", "itag 18 is picture and sound");
assert.equal(bdmMediaKind("https://example.org/a.mp4"), "", "a plain file is not a stream");
assert.equal(bdmMediaSize(ranged), 24117248);

assert.equal(bdmMediaLabel(ranged), "MP4");
assert.equal(bdmMediaLabel(audio), "AUDIO");
assert.equal(bdmMediaLabel("https://example.org/a.webm"), "WEBM");
assert.equal(bdmMediaName("https://example.org/a.mp4", "Page"), "a.mp4");
assert.equal(bdmMediaName(ranged, "Film"), "Film · 1080p video only · 23.0 MB");
assert.equal(bdmMediaName(ranged, ""), "1080p video only · 23.0 MB");
assert.equal(bdmMediaName(audio, "Film"), "Film · audio");
assert.equal(bdmMediaExtension(ranged), "mp4");
assert.equal(bdmMediaExtension(audio), "webm");
assert.equal(bdmMediaFileName(ranged, "My / Film: 1"), "My Film 1 1080p.mp4");
assert.equal(bdmMediaFileName("https://example.org/a.webm?sig=1", "Page"), "a.webm");

/* ----------------------------------------------------------- page-media.js */

const pageVideo = {
  currentSrc: "blob:https://example.org/abc",
  src: "",
  mediaKeys: null,
  querySelectorAll: () => [],
};
const pageAudio = {
  currentSrc: "https://cdn.example.org/song.mp3",
  src: "",
  mediaKeys: null,
  querySelectorAll: () => [],
};
const pageContext = vm.createContext({
  URL,
  console,
  performance: {
    getEntriesByType: () => [
      { name: ranged },
      { name: ranged.replace("range=0-1048575", "range=1048576-2097151") },
      { name: audio },
      { name: ranged.replace("itag=137", "itag=18") },
      { name: "https://example.org/app.js" },
    ],
  },
  PerformanceObserver: class {
    observe() {}
  },
  document: {
    title: "A Test Video - YouTube",
    querySelectorAll: (selector) => (selector === "video, audio" ? [pageVideo, pageAudio] : []),
  },
  location: { href: "https://example.org/watch" },
});
vm.runInContext(read("lib/manifest.js"), pageContext, { filename: "manifest.js" });
vm.runInContext(read("lib/media.js"), pageContext, { filename: "media.js" });
vm.runInContext(read("content/page-media.js"), pageContext, { filename: "page-media.js" });
pageContext.bdmWatchMediaRequests();

const listed = pageContext.bdmPageMedia();
assert.equal(listed.length, 4, "the song and the three distinct streams are listed");
assert.deepEqual(
  Array.from(listed, (item) => item.label),
  ["MP3", "MP4", "AUDIO", "MP4"],
);
assert.equal(listed[1].name, "A Test Video · 1080p video only · 23.0 MB");
assert.equal(listed[1].file, "A Test Video 1080p.mp4");
assert.equal(listed[2].file, "A Test Video audio.webm");
assert.ok(
  listed.every((item) => !/[?&](range|rn|rbuf)=/.test(item.url)),
  "no range request is handed over",
);
assert.equal(pageContext.bdmMediaCount(), 4, "the count matches the list");