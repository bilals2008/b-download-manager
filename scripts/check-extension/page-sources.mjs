// content/page-sources.js: Facebook and YouTube sources, and segment capture.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read } from "./common.mjs";

const facebookScript = {
  textContent: JSON.stringify({
    playable_url: "https://video.example.fbcdn.net/sd.mp4?efg=1&amp;x=1",
    playable_url_quality_hd: "https://video.example.fbcdn.net/hd.mp4?efg=2",
    browser_native_hd_url: "https://video.example.fbcdn.net/hd.mp4?efg=2",
  }),
};
const facebookContext = vm.createContext({
  URL,
  console,
  document: {
    title: "A Facebook video",
    scripts: [facebookScript],
    querySelectorAll: () => [],
  },
  location: {
    href: "https://www.facebook.com/watch/?v=1",
    hostname: "www.facebook.com",
  },
});
vm.runInContext(read("lib/manifest.js"), facebookContext, { filename: "manifest.js" });
vm.runInContext(read("lib/media.js"), facebookContext, { filename: "media.js" });
vm.runInContext(read("content/page-media.js"), facebookContext, { filename: "page-media.js" });
vm.runInContext(read("content/page-sources.js"), facebookContext, { filename: "page-sources.js" });

const facebookSources = facebookContext.bdmPageSources();
assert.equal(facebookSources.length, 2, "the HD and SD progressive files are read");
assert.deepEqual(
  Array.from(facebookSources, (source) => source.label),
  ["HD", "Video"],
);
assert.ok(
  facebookSources.every((source) => source.fullFile),
  "a progressive Facebook address is a complete file",
);
assert.equal(
  facebookContext.bdmPageSourceLabel("https://video.example.fbcdn.net/hd.mp4?efg=2"),
  "HD",
);
assert.equal(
  facebookContext.bdmPageSourceIsFullFile(
    "https://video.example.fbcdn.net/sd.mp4?efg=1&x=1",
  ),
  true,
  "an HTML-escaped ampersand is read as an address",
);
assert.equal(facebookContext.bdmPageSourceName(facebookSources[0].url), "A Facebook video");

const youtubeResponse = {
  streamingData: {
    formats: [
      {
        itag: 18,
        height: 360,
        mimeType: "video/mp4",
        url: "https://rr.example.googlevideo.com/videoplayback?itag=18",
      },
    ],
    adaptiveFormats: [
      {
        itag: 137,
        height: 1080,
        mimeType: "video/mp4",
        url: "https://rr.example.googlevideo.com/videoplayback?itag=137",
      },
      {
        itag: 140,
        mimeType: "audio/mp4",
        url: "https://rr.example.googlevideo.com/videoplayback?itag=140",
      },
    ],
  },
};
const youtubeContext = vm.createContext({
  URL,
  console,
  document: {
    title: "A Test Video - YouTube",
    scripts: [
      { textContent: `var ytInitialPlayerResponse = ${JSON.stringify(youtubeResponse)};` },
    ],
    querySelectorAll: () => [],
  },
  location: {
    href: "https://www.youtube.com/watch?v=abc",
    hostname: "www.youtube.com",
  },
});
vm.runInContext(read("lib/manifest.js"), youtubeContext, { filename: "manifest.js" });
vm.runInContext(read("lib/media.js"), youtubeContext, { filename: "media.js" });
vm.runInContext(read("content/page-media.js"), youtubeContext, { filename: "page-media.js" });
vm.runInContext(read("content/page-sources.js"), youtubeContext, { filename: "page-sources.js" });

const youtubeSources = youtubeContext.bdmPageSources();
assert.deepEqual(
  Array.from(youtubeSources, (source) => source.label),
  ["1080p", "360p", "audio"],
  "YouTube's own player response gives the streams and their labels",
);
assert.equal(youtubeContext.bdmPageHasFullFiles(), false, "a YouTube stream is not a whole file");

const segmentContext = vm.createContext({
  URL,
  console,
  performance: {
    getEntriesByType: () => [
      { name: "https://video.example.fbcdn.net/segment-1.mp4?efg=9" },
      { name: "https://video.example.fbcdn.net/segment-2.mp4?efg=9" },
    ],
  },
  PerformanceObserver: class {
    observe() {}
  },
  document: {
    title: "A Facebook video",
    scripts: [facebookScript],
    querySelectorAll: () => [],
  },
  location: {
    href: "https://www.facebook.com/watch/?v=1",
    hostname: "www.facebook.com",
  },
});
vm.runInContext(read("lib/manifest.js"), segmentContext, { filename: "manifest.js" });
vm.runInContext(read("lib/media.js"), segmentContext, { filename: "media.js" });
vm.runInContext(read("content/page-media.js"), segmentContext, { filename: "page-media.js" });
vm.runInContext(read("content/page-sources.js"), segmentContext, { filename: "page-sources.js" });
segmentContext.bdmWatchMediaRequests();

const capturedOnFacebook = segmentContext.bdmCapturedSources();
assert.ok(
  capturedOnFacebook.every((url) => !url.includes("segment-")),
  "the pieces of a Facebook stream are not offered as files",
);
assert.ok(
  capturedOnFacebook.some((url) => url.includes("hd.mp4")),
  "the complete HD file is offered instead",
);