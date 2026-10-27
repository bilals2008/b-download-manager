// Self-check for the File Exists and Download Failed windows: the question the
// engine's refusal becomes, the answers a conflict allows, and the sentence a
// failure is reported in. Node strips the types, so this runs against the same
// modules the windows import. Run it with `npm run check:recovery`.

import assert from "node:assert/strict";

import {
  answerAllowed,
  applyToRemainingLabel,
  canApplyToRemaining,
  conflictFrom,
  isStartRefusal,
  StartCancelled,
  startResolving,
} from "../src/lib/conflict.ts";
import {
  explainFailure,
  receivedSummary,
  resumeSummary,
} from "../src/lib/failures.ts";

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

function matches(value, pattern, label) {
  assert.match(value, pattern, label);
  checks += 1;
}

// The question: the file and the folder are the ones the engine refused, and
// the free name is offered without the full path in front of it.
const exists = conflictFrom(
  {
    kind: "exists",
    message: "A file with this name is already there.",
    path: "D:\\Downloads\\film.mp4",
    suggested: "D:\\Downloads\\film (2).mp4",
  },
  0,
);
is(exists.name, "film.mp4", "the file that was in the way");
is(exists.folder, "D:\\Downloads", "the folder the file is in");
is(exists.suggestedName, "film (2).mp4", "the free name beside it");
is(exists.active, false, "a file on disk is not an active download");
is(canApplyToRemaining(exists), false, "one file has no batch to answer for");

const active = conflictFrom(
  {
    kind: "active",
    message: "Another download is already writing to this file.",
    path: "D:\\Downloads\\film.mp4",
    suggested: "D:\\Downloads\\film (2).mp4",
  },
  3,
);
is(active.active, true, "a running download holds the file");
is(canApplyToRemaining(active), true, "the rest of the batch can use the answer");
is(applyToRemainingLabel(3), "Apply to the remaining 3 files", "batch label");
is(applyToRemainingLabel(1), "Apply to the remaining file", "one file left");
is(applyToRemainingLabel(0), "", "nothing left to apply to");

// The four answers: Replace is the one a running download forbids, because it
// would pull the file out from under the other transfer.
is(answerAllowed(exists, "replace"), true, "a file on disk may be replaced");
is(answerAllowed(active, "replace"), false, "a running download may not");
is(answerAllowed(active, "rename"), true, "a new name is always allowed");
is(answerAllowed(active, "skip"), true, "skipping is always allowed");

// A missing suggestion is no suggestion, not the same name twice.
const noSuggestion = conflictFrom(
  { kind: "exists", message: "", path: "film.mp4", suggested: "film.mp4" },
  0,
);
is(noSuggestion.folder, ".", "a bare name has no folder");
is(noSuggestion.suggestedName, "", "the same name is not offered as new");

// The failure sentence: what happened, and what to do about it, without
// showing the raw engine message.
const network = explainFailure(
  "segment 3 failed after 5 attempts: error sending request for url (https://example.org/film.zip)",
);
is(network.kind, "network", "a dropped connection");
is(network.retryable, true, "a connection can come back");
matches(network.hint, /connection/i, "the hint names the connection");

is(
  explainFailure(
    "segment 0 failed: the server refused the request (403); the link may have expired",
  ).kind,
  "refused",
  "a refused request",
);
is(
  explainFailure("segment 0 failed: the file is not on the server any more (404)")
    .kind,
  "missing",
  "a missing file",
);
is(
  explainFailure("segment 1 failed: the server is busy (429) and asked us to come back later")
    .kind,
  "busy",
  "a busy server",
);
is(
  explainFailure("not enough space on the disk: 1024 bytes free, 2048 needed").kind,
  "full",
  "a full disk",
);
is(
  explainFailure("Permission denied (os error 13)").kind,
  "permission",
  "a folder that cannot be written",
);
is(
  explainFailure(
    "D:\\Downloads\\film.mp4 appeared while the download was running; it was not replaced",
  ).kind,
  "changed",
  "a file that appeared meanwhile",
);
is(
  explainFailure(
    "D:\\Downloads\\film.mp4 appeared while the download was running; it was not replaced",
  ).retryable,
  false,
  "retrying the same destination cannot fix a file that appeared",
);
is(
  explainFailure("the assembled file is 10 bytes, the server promised 20").kind,
  "size",
  "a short assembly",
);
is(explainFailure(null).kind, "unknown", "a failure without a sentence");
is(explainFailure(null).headline, "The download could not finish.", "fallback headline");

// The state row: what arrived, and whether a retry continues from it.
is(receivedSummary({ downloaded: 0, size: 100, status: "error" }), "Nothing has arrived yet.");
is(
  receivedSummary({ downloaded: 25, size: 100, status: "error" }),
  "25% received",
  "a fraction of the file",
);
is(
  receivedSummary({ downloaded: 25, size: 0, status: "error" }),
  "Part of the file arrived, but the site did not say how big it is.",
  "an unknown size",
);
is(
  receivedSummary({ downloaded: 100, size: 100, status: "completed" }),
  "The file is complete.",
  "a finished file",
);
matches(resumeSummary(true), /already downloaded/, "a resumable retry");
matches(resumeSummary(false), /beginning/, "a retry that starts over");
matches(resumeSummary(null), /works out/, "a retry the app works out");

// The dismissal is a real error type, so a start can tell it apart from a
// refusal the window still has to read.
is(new StartCancelled() instanceof Error, true, "the dismissal is an error");
is(new StartCancelled().name, "StartCancelled", "the dismissal names itself");

// The start loop: no question when the name is free, one question when it is
// not, and never a second question about an answer that already failed.
function refusal(kind = "exists") {
  return {
    kind,
    message: "A file with this name is already there.",
    path: "D:\\Downloads\\film.mp4",
    suggested: "D:\\Downloads\\film (2).mp4",
  };
}

const started = (id) => ({
  id,
  dest: "D:\\Downloads\\film.mp4",
  skipped: false,
});

is(isStartRefusal(refusal()), true, "the engine's refusal is recognised");
is(isStartRefusal(new Error("boom")), false, "an error is not a refusal");
is(isStartRefusal({ kind: "exists" }), false, "a refusal needs a suggestion");

{
  const choices = [];
  let questions = 0;
  const outcome = await startResolving(
    async (choice) => {
      choices.push(choice);
      return started(1);
    },
    async () => {
      questions += 1;
      return null;
    },
  );
  is(outcome.id, 1, "a free destination starts at once");
  is(choices, ["ask"], "the engine is asked before the window");
  is(questions, 0, "a free destination asks nothing");
}

{
  const choices = [];
  const questions = [];
  const outcome = await startResolving(
    async (choice) => {
      choices.push(choice);
      if (choice === "ask") {
        throw refusal();
      }
      return {
        id: 7,
        dest: "D:\\Downloads\\film (2).mp4",
        skipped: false,
      };
    },
    async (question) => {
      questions.push(question);
      return { choice: "rename", applyToRemaining: false };
    },
  );
  is(choices, ["ask", "rename"], "the answer is the second attempt");
  is(questions.length, 1, "one question");
  is(questions[0].suggestedName, "film (2).mp4", "the question offers the free name");
  is(outcome.id, 7, "the renamed download starts");
  is(outcome.dest, "D:\\Downloads\\film (2).mp4", "the row follows the new name");
}

await assert.rejects(
  () =>
    startResolving(
      async () => {
        throw refusal();
      },
      async () => null,
    ),
  StartCancelled,
  "a dismissed question cancels the start",
);
checks += 1;

{
  let attempts = 0;
  let questions = 0;
  let caught = null;
  try {
    await startResolving(
      async () => {
        attempts += 1;
        throw refusal();
      },
      async () => {
        questions += 1;
        return { choice: "rename", applyToRemaining: false };
      },
    );
  } catch (failure) {
    caught = failure;
  }
  is(caught?.kind, "exists", "a choice that still fails is reported");
  is(attempts, 2, "one attempt before the answer and one after it");
  is(questions, 1, "the window does not ask a second time");
}

{
  const batch = { remaining: 1, choice: null };
  const choices = [];
  let questions = 0;
  async function file(number) {
    return startResolving(
      async (choice) => {
        choices.push(`${number}:${choice}`);
        if (choice === "ask") {
          throw refusal();
        }
        return started(number);
      },
      async () => {
        questions += 1;
        return { choice: "rename", applyToRemaining: true };
      },
      batch,
    );
  }
  await file(1);
  batch.remaining = 0;
  await file(2);
  is(choices, ["1:ask", "1:rename", "2:rename"], "one answer covers the batch");
  is(questions, 1, "the second file does not ask again");
  is(batch.choice, "rename", "the answer is kept for the rest");
}

{
  const batch = { remaining: 0, choice: "skip" };
  let questions = 0;
  const outcome = await startResolving(
    async (choice) => {
      if (choice === "skip") {
        throw refusal("active");
      }
      return started(3);
    },
    async () => {
      questions += 1;
      return { choice: "rename", applyToRemaining: false };
    },
    batch,
  );
  is(questions, 1, "an answer that did not settle asks again");
  is(batch.choice, null, "that answer is dropped for the rest");
  is(outcome.id, 3, "the file the user answered for starts");
}

{
  let caught = null;
  let questions = 0;
  try {
    await startResolving(
      async () => {
        throw { kind: "invalid", message: "a URL is required" };
      },
      async () => {
        questions += 1;
        return null;
      },
    );
  } catch (failure) {
    caught = failure;
  }
  is(caught?.message, "a URL is required", "an invalid start is reported");
  is(questions, 0, "an invalid start is not asked about");
}

console.log(`check:recovery — ${checks} checks passed`);
