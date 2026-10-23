// Self-check for the add download dialog: the address, the file name and the
// destination the dialog builds before the engine sees them. Node strips the
// types, so this runs against the same helpers the dialog imports.
// Run it with `npm run check:add-dialog`.

import assert from "node:assert/strict";

import {
  categoryName,
  categoryOfFile,
  downloadUrl,
  fileNameFromUrl,
  fileNameProblem,
  formatSize,
  joinPath,
  sanitizeFileName,
  splitPath,
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

console.log(`add download dialog: ${checks} checks pass`);
