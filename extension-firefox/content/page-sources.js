/*
  Media a page already knows about.

  Two players hide their streams from the DOM: YouTube builds a blob: source
  and Facebook streams one small request per piece. The addresses themselves
  are still written into the page's own scripts:

  - YouTube keeps `ytInitialPlayerResponse`, with one address per itag.
  - Facebook keeps full progressive MP4 addresses (`playable_url_quality_hd`,
    `browser_native_hd_url`, ...), each a complete file with its sound.

  Reading those once per page is cheaper and far more accurate than offering
  every segment the player fetched. A source that is a complete file is marked
  `fullFile`; the overlay hands it straight to the app instead of asking the app
  to parse a fragment. Read-only and site-scoped: a page of another site gets no
  source from here.
*/

const BDM_PAGE_SOURCE_LIMIT = 16;
const BDM_PAGE_SCRIPT_LIMIT = 4_000_000;

const pageSourceCache = {
  key: "",
  sources: [],
};

function bdmPageSourceKey() {
  return `${String(location.href || "").split("#")[0]}|${document.scripts?.length ?? 0}`;
}

function bdmUnescapeJsonString(raw) {
  try {
    return JSON.parse(`"${raw}"`);
  } catch {
    return String(raw).replace(/\\\//g, "/");
  }
}

function bdmSiteIs(host, name) {
  return host === name || host.endsWith(`.${name}`);
}

/** Both labels of one address: a plain copy does not outrank its HD one. */
function bdmAddPageSource(found, url, label, fullFile) {
  if (!/^https?:\/\//i.test(url)) return;
  const rank = label === "HD" ? 3 : label === "Video" ? 2 : 1;
  const previous = found.get(url);
  if (previous && previous.rank >= rank) return;
  found.set(url, { url, label, fullFile, rank });
}

const FACEBOOK_FIELDS = [
  ["playable_url_quality_hd", "HD"],
  ["browser_native_hd_url", "HD"],
  ["progressive_url_hd", "HD"],
  ["playable_url", "Video"],
  ["progressive_url", "Video"],
  ["playable_url_quality_sd", "SD"],
  ["browser_native_sd_url", "SD"],
  ["progressive_url_sd", "SD"],
];

/** The full progressive files a Facebook page keeps in its own scripts. */
function bdmFacebookSources(scripts) {
  const found = new Map();
  for (const text of scripts) {
    if (!/playable_url|browser_native|progressive_url/.test(text)) continue;
    for (const [field, label] of FACEBOOK_FIELDS) {
      const pattern = new RegExp(
        `"${field}"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`,
        "g",
      );
      let match;
      while ((match = pattern.exec(text))) {
        const url = bdmUnescapeJsonString(match[1]).replace(/&amp;/g, "&");
        // A progressive address is a whole MP4, not a piece of a stream.
        if (!/\.mp4(?:[?#]|$)/i.test(url)) continue;
        bdmAddPageSource(found, url, label, true);
      }
    }
  }
  return [...found.values()];
}

/** The object after `marker =`, with nested braces and strings read correctly. */
function bdmBalancedObject(text, start) {
  let depth = 0;
  let string = false;
  let escaped = false;
  for (let index = start; index < text.length; index += 1) {
    const char = text[index];
    if (string) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === '"') string = false;
      continue;
    }
    if (char === '"') string = true;
    else if (char === "{") depth += 1;
    else if (char === "}") {
      depth -= 1;
      if (depth === 0) return text.slice(start, index + 1);
    }
  }
  return "";
}

/** The streaming formats inside YouTube's own player response. */
function bdmYouTubeSources(scripts) {
  let response = null;
  // YouTube also keeps the object on the page window. Firefox content scripts
  // reach it through the wrapped page object; other browsers only offer the
  // script text below.
  try {
    const pageWindow =
      typeof window === "undefined"
        ? null
        : window.wrappedJSObject || window;
    const direct = pageWindow?.ytInitialPlayerResponse;
    if (direct?.streamingData) response = direct;
  } catch {
    // A compartment that refuses the read leaves the script text below.
  }
  for (const text of scripts) {
    if (response || !text.includes("ytInitialPlayerResponse")) continue;
    let at = text.indexOf("ytInitialPlayerResponse");
    while (at !== -1 && !response) {
      const brace = text.indexOf("{", at);
      if (brace !== -1) {
        const json = bdmBalancedObject(text, brace);
        if (json) {
          try {
            const parsed = JSON.parse(json);
            if (parsed?.streamingData) response = parsed;
          } catch {
            // The next assignment on the page may be the readable one.
          }
        }
      }
      at = text.indexOf("ytInitialPlayerResponse", at + 1);
    }
  }
  if (!response) return [];

  const formats = [
    ...(response.streamingData.formats || []),
    ...(response.streamingData.adaptiveFormats || []),
  ];
  const videos = formats
    .filter((format) => Number.isFinite(format.height) && format.height > 0)
    .sort((left, right) => right.height - left.height)
    .slice(0, 10);
  const audios = formats
    .filter(
      (format) =>
        !format.height && /^audio\//i.test(String(format.mimeType || "")),
    )
    .sort((left, right) => Number(right.bitrate || 0) - Number(left.bitrate || 0))
    .slice(0, 2);
  const found = new Map();
  for (const format of [...videos, ...audios]) {
    const url = typeof format.url === "string" ? format.url : "";
    if (!url) continue;
    const audio = !format.height;
    bdmAddPageSource(
      found,
      url,
      audio ? "audio" : `${format.height}p`,
      false,
    );
  }
  return [...found.values()];
}

/**
 * The complete files and streams this page already names.
 *
 * The answer is cached per address and script count: a player adds the data
 * after the document loads, and a navigation inside the page changes the
 * address, so neither can keep a stale list alive.
 */
function bdmPageSources() {
  const key = bdmPageSourceKey();
  if (key === pageSourceCache.key) return pageSourceCache.sources;

  const host = String(location.hostname || "").toLowerCase();
  const scripts = [...(document.scripts || [])]
    .map((script) => String(script.textContent || ""))
    .filter(Boolean)
    .slice(0, 80);
  let length = 0;
  const readable = [];
  for (const text of scripts) {
    if (length > BDM_PAGE_SCRIPT_LIMIT) break;
    if (
      !/playable_url|browser_native|progressive_url|ytInitialPlayerResponse/.test(
        text,
      )
    ) {
      continue;
    }
    readable.push(text);
    length += text.length;
  }

  let sources = [];
  if (bdmSiteIs(host, "facebook.com") || bdmSiteIs(host, "fb.watch")) {
    sources = bdmFacebookSources(readable);
  } else if (bdmSiteIs(host, "youtube.com") || bdmSiteIs(host, "youtu.be")) {
    sources = bdmYouTubeSources(readable);
  }

  pageSourceCache.key = key;
  pageSourceCache.sources = sources.slice(0, BDM_PAGE_SOURCE_LIMIT);
  return pageSourceCache.sources;
}

function bdmPageSource(url) {
  return bdmPageSources().find((source) => source.url === url) || null;
}

function bdmPageSourceLabel(url) {
  return bdmPageSource(url)?.label || "";
}

/** The name a page source should be saved as, when the address cannot name it. */
function bdmPageSourceName(url) {
  const source = bdmPageSource(url);
  return source?.fullFile ? bdmPageName() : "";
}

function bdmPageSourceIsFullFile(url) {
  return Boolean(bdmPageSource(url)?.fullFile);
}

function bdmPageHasFullFiles() {
  return bdmPageSources().some((source) => source.fullFile);
}
