// Self-check for the Add URL dialog: the address, the file name, the reason a
// probe failed and the destination the dialog builds before the engine sees
// them. Node strips the types, so this runs against the same helpers the
// dialog imports. Run it with `npm run check:add-dialog`.

import assert from "node:assert/strict";

import {
  categoryName,
  categoryOfFile,
  downloadUrl,
  fileNameFromUrl,
  fileNameProblem,
  formatSize,
  joinPath,
  MAX_FILE_NAME,
  probeWarning,
  safeFileName,
  sameAddress,
  sanitizeFileName,
  splitPath,
  urlProblem,
} from "../src/lib/downloads.ts";

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

// The address: the schemes the engine fetches, a host, and no raw spaces.
is(downloadUrl("https://example.org/a.zip")?.protocol, "https:", "https");
is(downloadUrl(" http://example.org/a.zip ")?.protocol, "http:", "http");
is(downloadUrl("ftp://example.org/a.zip")?.protocol, "ftp:", "ftp");
is(downloadUrl("magnet:?xt=urn:btih:1"), null, "a magnet link is not a file");
is(downloadUrl("file:///c:/windows"), null, "a local file is not a download");
is(downloadUrl("javascript:alert(1)"), null, "a script is not a download");
is(downloadUrl("https://"), null, "an address without a host");
is(downloadUrl(""), null, "an empty address");
is(downloadUrl("https://example.org/a b.zip"), null, "a raw space");

// The line beside the field says what to change, not just that it is wrong.
is(urlProblem("https://example.org/a.zip"), null, "a usable address");
is(urlProblem(" ftp://example.org/a.zip "), null, "surrounding spaces are trimmed");
is(
  urlProblem(""),
  "Paste the address of the file to download.",
  "an empty address",
);
is(
  urlProblem("https://example.org/a b.zip"),
  "The address cannot contain spaces.",
  "a raw space explains itself",
);
is(
  urlProblem("example.org/a.zip"),
  "Enter a full address, starting with http://, https:// or ftp://.",
  "an address without a scheme",
);
is(
  urlProblem("magnet:?xt=urn:btih:1"),
  "Only http, https and ftp addresses can be downloaded.",
  "an unsupported scheme",
);
is(
  urlProblem("file:///c:/windows"),
  "Only http, https and ftp addresses can be downloaded.",
  "a local file",
);

// The duplicate note compares what the address points at, not how it was
// typed: the host's case and a default port are the same link.
is(
  sameAddress("HTTPS://Example.org/a.zip", "https://example.org/a.zip"),
  true,
  "scheme and host case",
);
is(
  sameAddress("https://example.org:443/a.zip", "https://example.org/a.zip"),
  true,
  "a default port",
);
is(
  sameAddress("https://example.org/a.zip", "https://example.org/b.zip"),
  false,
  "two different files",
);
is(sameAddress("not a url", "not a url"), true, "a broken address still compares");

// A refused check is advice in one line: the download is still allowed.
is(
  probeWarning("the server asked us to sign in (401)"),
  "The server asks for a sign-in. The download may fail without one.",
  "authentication",
);
is(
  probeWarning("the server answered 407"),
  "The proxy asks for a sign-in, so it refused the check.",
  "proxy authentication",
);
is(
  probeWarning(
    "the server refused the request (403); the link may have expired, or the site only answers a browser",
  ),
  "The server refused the check. The link may have expired, or the site may only answer a browser.",
  "a refusal",
);
is(
  probeWarning("the file is not on the server any more (404)"),
  "The file is not on the server any more.",
  "a missing file",
);
is(
  probeWarning("cannot reach the server: error sending request"),
  "The server could not be reached. Check the address and your connection.",
  "an unreachable server",
);
is(
  probeWarning("error following redirect"),
  "The address redirected too many times.",
  "a redirect loop",
);
is(
  probeWarning("the server is busy (429) and asked us to come back later"),
  "The server is busy. Try again in a moment.",
  "a busy server",
);
is(
  probeWarning("the server had a problem (500)"),
  "The server had a problem. Try again later.",
  "a failing server",
);
is(
  probeWarning(""),
  "The link could not be checked. The download may still work.",
  "an empty reason",
);
is(
  probeWarning("the server answered 418"),
  "The link could not be checked: the server answered 418",
  "an unexpected answer",
);

// The name: the fields of Windows a file name cannot hold, plus the device
// names, which the engine refuses too.
is(fileNameProblem("film.mp4"), null, "a plain name");
is(fileNameProblem("console.log"), null, "a name that only starts like a device");
is(fileNameProblem("  "), "Enter a file name.", "an empty name");
is(fileNameProblem("a:b.zip"), 'A file name cannot contain < > : " / \\ | ? *.');
is(fileNameProblem("a/b.zip"), 'A file name cannot contain < > : " / \\ | ? *.');
is(fileNameProblem("a".repeat(201)), "The file name is longer than 200 characters.");
is(fileNameProblem("CON.txt"), "CON.txt is a name Windows keeps for a device.");
is(fileNameProblem("lpt9"), "lpt9 is a name Windows keeps for a device.");
is(fileNameProblem("report.pdf."), "A file name cannot end with a dot or a space.");
is(fileNameProblem("report.pdf "), "A file name cannot end with a dot or a space.");
is(fileNameProblem(".."), "That is not a file name.");

// The name the address carries, decoded, and the fallback when it carries none.
is(
  fileNameFromUrl("https://example.org/files/a%20b.zip?token=1"),
  "a b.zip",
  "a name from the path, without the query",
);
is(fileNameFromUrl("https://example.org/"), "download", "a folder has no name");
is(fileNameFromUrl("not a url"), "download", "a broken address");

// A typed name is made safe, never left with a separator in it.
is(sanitizeFileName('a<b>.zip'), "a_b_.zip", "characters a path cannot hold");
is(sanitizeFileName("  film.mp4.  "), "film.mp4", "trailing dots and spaces");
is(sanitizeFileName("   "), "download", "an empty name");

// The name the dialog starts with always reaches the engine: the field is not
// editable, so a device name or a name that is too long has to be repaired
// here rather than refused after the click.
is(safeFileName("CON.txt"), "_CON.txt", "a device name gets out of the way");
is(safeFileName("lpt9"), "_lpt9", "a device name without an extension");
is(safeFileName("  film.mp4.  "), "film.mp4", "the sanitizer runs first");
is(safeFileName("   "), "download", "a name is always there");
const longName = safeFileName(`${"a".repeat(240)}.zip`);
is(longName.length, MAX_FILE_NAME, "a long name is cut to the engine's limit");
is(longName.endsWith(".zip"), true, "the extension survives the cut");
is(
  safeFileName("a".repeat(240)),
  "a".repeat(MAX_FILE_NAME),
  "a long name without an extension",
);

// The category the dialog shows, and the one a row restored from disk keeps.
is(categoryOfFile("x.MP4"), "Video");
is(categoryOfFile("x.unknown"), "Other");
is(categoryName("Video"), "Video");
is(categoryName("video"), null, "the stored names are exact");
is(categoryName(null), null);

// The destination is the folder plus the name, with the folder's separator.
is(joinPath("C:\\Downloads", "a.zip"), "C:\\Downloads\\a.zip");
is(joinPath("C:/Downloads/", "a.zip"), "C:/Downloads/a.zip");
is(joinPath("", "a.zip"), "a.zip");
is(splitPath("C:\\Downloads\\a.zip"), {
  dir: "C:\\Downloads",
  base: "a.zip",
});
is(formatSize(1536), "1.50 KB");

console.log(`Add URL dialog: ${checks} checks pass`);
