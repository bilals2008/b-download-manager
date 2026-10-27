// Self-check for the properties window: which fields a row may move, which
// row has a file to delete, and the words the resume state is shown in. Node
// strips the types, so this runs against the same module the window imports.
// Run it with `npm run check:properties`.

import assert from "node:assert/strict";

import {
  canDeleteFile,
  destinationLock,
  fileNameProblem,
  joinPath,
  resumableLabel,
} from "../src/lib/downloads.ts";

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

// A task that is writing holds its file, so the name and the folder wait.
for (const status of ["downloading", "queued"]) {
  const reason = destinationLock({ status });
  is(typeof reason, "string", `${status} fixes the save location`);
  assert.ok(reason.length > 0, `${status} explains why`);
  checks += 1;
}

// Everything else may move: a paused or stopped row has only parts, a failed
// one has parts and a message, and a finished one has the file itself.
for (const status of ["paused", "stopped", "error", "completed"]) {
  is(destinationLock({ status }), null, `${status} may change its location`);
}

is(
  destinationLock({ status: "downloading" }),
  "Stop the download before changing where it is saved.",
  "a running row is stopped first",
);
is(
  destinationLock({ status: "queued" }),
  "Take the download out of the queue before changing where it is saved.",
  "a queued row leaves the queue first",
);

// Removing a row from the list is not deleting its file: only a finished row
// has one, and only that row may offer the command.
for (const status of ["downloading", "queued", "paused", "stopped", "error"]) {
  is(canDeleteFile({ status }), false, `${status} has no file to delete`);
}
is(canDeleteFile({ status: "completed" }), true, "a finished file can be deleted");

// The resume line explains both answers, and does not pretend to know before
// the site has answered.
is(resumableLabel(true), "Yes, it picks up where it stopped");
is(resumableLabel(false), "No, it starts the file over");
is(resumableLabel(null), "Not known yet");
checks += 3;

// The path the window sends is the folder and the name, joined the way the
// folder is written.
is(joinPath("D:\\Downloads", "film.mp4"), "D:\\Downloads\\film.mp4");
is(joinPath("D:\\Downloads\\", "film.mp4"), "D:\\Downloads\\film.mp4");
is(joinPath("/home/user", "film.mp4"), "/home/user/film.mp4");
checks += 3;

// A name the engine would refuse is caught beside the field, not as a failed
// move.
is(fileNameProblem("film.mp4"), null, "a plain name is accepted");
is(
  fileNameProblem("CON.txt"),
  "CON.txt is a name Windows keeps for a device.",
  "a device name is refused",
);
is(
  fileNameProblem("film.mp4."),
  "A file name cannot end with a dot or a space.",
  "a trailing dot is refused",
);
is(
  fileNameProblem("a:b.mp4"),
  'A file name cannot contain < > : " / \\ | ? *.',
  "a separator is refused",
);
is(fileNameProblem(""), "Enter a file name.", "an empty name is refused");

console.log(`check:properties — ${checks} checks passed`);
