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

/*
  YouTube hands over signed addresses in two shapes. A `url` field is already
  signed; a `signatureCipher` field names the base address and a scrambled
  signature that only the player script can unscramble. A format that carries
  only the cipher used to be dropped, which is why the button found nothing on
  many videos. This block reads the player script and runs its own decipher, the
  way the page does, so a cipher format becomes a real address.

  The decipher is best effort: a player rework that changes the function shape
  leaves it unread, and the base address is offered instead of nothing.
*/

const ytPlayerState = { key: "", ready: 0, decipher: null, nsolve: null, loading: false };

/** The address of the player script the page is using, from its own config. */
function bdmYoutubePlayerUrl(scripts) {
  for (const text of scripts) {
    const match =
      /"PLAYER_JS_URL":"([^"]+)"/.exec(text) || /"jsUrl":"([^"]+)"/.exec(text);
    if (match) {
      try {
        return new URL(match[1], location.origin).href;
      } catch {
        // A malformed address is skipped.
      }
    }
  }
  if (typeof document.querySelector === "function") {
    const script = document.querySelector('script[src*="/s/player/"][src*="base.js"]');
    if (script) {
      try {
        return new URL(script.src, location.origin).href;
      } catch {
        // A malformed address is skipped.
      }
    }
  }
  return "";
}

/** Fetch the player once and build its decipher. */
function bdmYoutubeLoadPlayer(url) {
  if (typeof fetch !== "function") return;
  if (!url || ytPlayerState.loading || ytPlayerState.key === url) return;
  ytPlayerState.key = url;
  ytPlayerState.loading = true;
  fetch(url, { credentials: "omit" })
    .then((response) => response.text())
    .then((text) => {
      ytPlayerState.decipher = bdmYoutubeBuildDecipher(text);
      ytPlayerState.nsolve = bdmYoutubeBuildN(text);
      // A fresh solver invalidates the cached source list so the next read
      // offers the signed, unthrottled address rather than the bare base.
      ytPlayerState.ready += 1;
      pageSourceCache.key = "";
    })
    .catch(() => {
      // A player that cannot be read keeps the base address in use.
    })
    .finally(() => {
      ytPlayerState.loading = false;
    });
}

/** Turn a format into a real address: sign a cipher and fix the `n` throttle. */
function bdmYoutubeFormatUrl(format) {
  if (typeof format.url === "string" && format.url) return bdmYoutubeFixN(format.url);
  const cipher = format.signatureCipher || format.cipher;
  if (typeof cipher !== "string" || !cipher) return "";
  let params;
  try {
    params = new URLSearchParams(cipher);
  } catch {
    return "";
  }
  const base = params.get("url") || "";
  if (!/^https?:\/\//i.test(base)) return "";
  const scrambled = params.get("s");
  if (!scrambled) return bdmYoutubeFixN(base);
  const signature = bdmYoutubeDecipher(scrambled);
  if (!signature) return bdmYoutubeFixN(base);
  try {
    const address = new URL(base);
    address.searchParams.set(params.get("sp") || "signature", signature);
    return bdmYoutubeFixN(address.href);
  } catch {
    return bdmYoutubeFixN(base);
  }
}

function bdmYoutubeDecipher(value) {
  const decipher = ytPlayerState.decipher;
  if (typeof decipher !== "function" || !value) return "";
  try {
    const result = decipher(value);
    return typeof result === "string" ? result : "";
  } catch {
    return "";
  }
}

/** Find the signature decipher by testing candidates that split and rejoin. */
function bdmYoutubeBuildDecipher(text) {
  let at = text.indexOf('.split("")');
  while (at !== -1) {
    const start = Math.max(0, at - 6000);
    const windowText = text.slice(start, at + 6000);
    const local = at - start;
    let search = 0;
    while (search < local) {
      const fnAt = windowText.indexOf("function", search);
      if (fnAt < 0 || fnAt > local) break;
      const open = windowText.indexOf("{", fnAt);
      const close = bdmYoutubeBraceEnd(windowText, open);
      if (close > open && open < local && close > local) {
        const source = windowText.slice(fnAt, close + 1);
        if (source.includes('.split("")') && source.includes('.join("")')) {
          const compiled = bdmYoutubeCompile(text, source);
          if (compiled && bdmYoutubeLooksLikeDecipher(compiled)) return compiled;
        }
      }
      search = fnAt + 8;
    }
    at = text.indexOf('.split("")', at + 1);
  }
  return null;
}

/** A decipher keeps a string's length and changes its order. */
function bdmYoutubeLooksLikeDecipher(maybe) {
  try {
    const result = maybe("ABCDEFGHIJ");
    return typeof result === "string" && result.length === 10 && result !== "ABCDEFGHIJ";
  } catch {
    return false;
  }
}

/**
 * Evaluate a player function with every local helper it references in scope.
 *
 * The player keeps its helpers as arrays, objects and other functions in the
 * same script. Starting from the function, each local name it mentions is
 * pulled in as a `var`, and the definitions are followed in turn, so a cipher
 * or `n` routine runs exactly as it does on the page.
 */
function bdmYoutubeCompile(text, functionSource) {
  try {
    const visited = new Set();
    const queue = [functionSource];
    let prelude = "";
    while (queue.length) {
      const body = queue.shift();
      for (const name of new Set(body.match(/[A-Za-z_$][\w$]*/g) || [])) {
        if (visited.has(name)) continue;
        visited.add(name);
        const definition = bdmYoutubeDefinition(text, name);
        if (!definition) continue;
        prelude += `${definition};`;
        queue.push(definition);
        // A helper object often gets its methods after it is declared
        // (`X.reverse = function ...`), so those assignments come along too.
        const escaped = name.replace(/\$/g, "\\$");
        const member = new RegExp(`${escaped}\\.([A-Za-z_$][\\w$]*)\\s*=\\s*`, "g");
        let found;
        while ((found = member.exec(text))) {
          const value = bdmYoutubeValue(text, found.index + found[0].length);
          if (!value) continue;
          const assignment = `${name}.${found[1]}=${value}`;
          prelude += `${assignment};`;
          queue.push(assignment);
        }
      }
    }
    // eslint-disable-next-line no-new-func
    const factory = new Function(`${prelude} return (${functionSource});`);
    const compiled = factory();
    return typeof compiled === "function" ? compiled : null;
  } catch {
    // A shape this reader cannot build leaves the base address in use.
    return null;
  }
}

/** The local definition of `name`, as a `var name=...` fragment, or null. */
function bdmYoutubeDefinition(text, name) {
  const escaped = name.replace(/\$/g, "\\$");
  let match = new RegExp(`(?:var|let|const)\\s+${escaped}\\s*=\\s*`).exec(text);
  if (match) {
    const value = bdmYoutubeValue(text, match.index + match[0].length);
    return value ? `var ${name}=${value}` : null;
  }
  match = new RegExp(`function\\s+${escaped}\\s*\\(`).exec(text);
  if (match) {
    const value = bdmYoutubeFunctionSource(text, match.index);
    return value ? `var ${name}=${value}` : null;
  }
  return null;
}

/** Read one JS value (array, object or function) starting at `at`. */
function bdmYoutubeValue(text, at) {
  let index = at;
  while (index < text.length && /\s/.test(text[index])) index += 1;
  const char = text[index];
  if (char === "[" || char === "{") {
    const close = bdmYoutubeBraceEnd(text, index, char, char === "[" ? "]" : "}");
    return close > index ? text.slice(index, close + 1) : null;
  }
  if (text.startsWith("function", index)) {
    return bdmYoutubeFunctionSource(text, index);
  }
  return null;
}

/** A function expression from `at` (a `function` keyword or a named def). */
function bdmYoutubeFunctionSource(text, at) {
  const parenAt = text.indexOf("(", at);
  if (parenAt < 0) return null;
  const argsEnd = text.indexOf(")", parenAt);
  if (argsEnd < 0) return null;
  const open = text.indexOf("{", argsEnd);
  if (open < 0) return null;
  const close = bdmYoutubeBraceEnd(text, open);
  if (close < 0) return null;
  return `function${text.slice(parenAt, argsEnd + 1)}${text.slice(open, close + 1)}`;
}

/** Find and build the `n` transform the player applies to its addresses. */
function bdmYoutubeBuildN(text) {
  const name = bdmYoutubeNName(text);
  if (!name) return null;
  const escaped = name.replace(/\$/g, "\\$");
  let source = null;
  let match = new RegExp(`(?:var|let|const)\\s+${escaped}\\s*=\\s*`).exec(text);
  if (match) {
    source = bdmYoutubeValue(text, match.index + match[0].length);
  } else {
    match = new RegExp(`function\\s+${escaped}\\s*\\(`).exec(text);
    if (match) source = bdmYoutubeFunctionSource(text, match.index);
  }
  if (!source) return null;
  return bdmYoutubeCompile(text, source);
}

/** The name of the function the player calls on the `n` parameter. */
function bdmYoutubeNName(text) {
  const patterns = [
    /\.get\("n"\)\)&&\(b=([A-Za-z0-9_$]+)/,
    /b=String\.fromCharCode\(110\),c=a\.get\(b\)\)&&\(c=([A-Za-z0-9_$]+)/,
    /\.get\("n"\)\)&&\(b=([A-Za-z0-9_$]+)\[\d+\]/,
  ];
  for (const pattern of patterns) {
    const match = pattern.exec(text);
    if (match) return match[1];
  }
  return "";
}

/** Apply the `n` transform to an address, so the server does not throttle it. */
function bdmYoutubeFixN(url) {
  const solve = ytPlayerState.nsolve;
  if (typeof solve !== "function") return url;
  try {
    const address = new URL(url);
    const current = address.searchParams.get("n");
    if (!current) return url;
    const fixed = solve(current);
    if (typeof fixed === "string" && fixed && fixed !== current) {
      address.searchParams.set("n", fixed);
      return address.href;
    }
  } catch {
    // A transform that throws keeps the address it was given.
  }
  return url;
}

function bdmYoutubeBraceEnd(text, open, openCh = "{", closeCh = "}") {
  if (open < 0) return -1;
  let depth = 0;
  let quote = "";
  let escaped = false;
  for (let index = open; index < text.length; index += 1) {
    const char = text[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === quote) quote = "";
      continue;
    }
    if (char === '"' || char === "'" || char === "`") {
      quote = char;
      continue;
    }
    if (char === openCh) depth += 1;
    else if (char === closeCh) {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function bdmPageSourceKey() {
  return `${String(location.href || "").split("#")[0]}|${document.scripts?.length ?? 0}|${ytPlayerState.ready}`;
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
  // Facebook often repeats one address per byte range, all under the same
  // field. One row per quality is the useful list: the largest field it named
  // (HD), then the plain file, then SD.
  const byLabel = new Map();
  for (const source of found.values()) {
    if (!byLabel.has(source.label)) {
      byLabel.set(source.label, source);
    }
  }
  return [...byLabel.values()];
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
  // The application resolves a YouTube page through its own player client, so
  // the page itself does not fetch or evaluate the player script here.
  for (const format of [...videos, ...audios]) {
    const url = bdmYoutubeFormatUrl(format);
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
