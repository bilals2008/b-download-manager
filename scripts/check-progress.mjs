// Self-check for the download progress window: the percentage, the words, the
// transfer rate and the speed limiter conversions. Node strips the types, so
// the check runs against the same files the window imports.
// Run it with `npm run check:progress`.

import assert from "node:assert/strict";

import { etaSeconds, nextSpeedSample } from "../src/lib/speed.ts";
import {
  bytesToLimit,
  connectionLabel,
  connectionSummary,
  KB,
  limitLabel,
  limitToBytes,
  MB,
  maxLimitAmount,
  parseLimitAmount,
  percentOf,
  percentText,
  progressChunks,
  progressControls,
  progressPercentText,
  receivingCount,
  resumeLabel,
  segmentSize,
  statusLabel,
} from "../src/lib/progress.ts";

// The percentage: a finished download is all of itself, an unknown size has
// nothing to divide, and a live one is its share of the total.
assert.equal(
  percentOf({ status: "completed", total: 4096, downloaded: 0 }),
  100,
  "a finished download is 100% even after its ranges were joined",
);
assert.equal(
  percentOf({ status: "running", total: 0, downloaded: 2048 }),
  0,
  "a file of an unknown size has no percentage",
);
assert.equal(
  percentOf({ status: "running", total: 8192, downloaded: 2048 }),
  25,
  "a live download is its share of the total",
);
assert.equal(percentText(31.554), "31.55%");
assert.equal(percentText(100), "100%");
assert.equal(percentText(0), "0.00%");

// The percentage the window prints: a dash, not a guess, while the size is
// unknown, and 100% for a finished file whatever the server told us.
assert.equal(
  progressPercentText({ status: "running", total: 8192, downloaded: 2048 }),
  "25.00%",
);
assert.equal(
  progressPercentText({ status: "running", total: 0, downloaded: 2048 }),
  "—",
);
assert.equal(
  progressPercentText({ status: "completed", total: 0, downloaded: 2048 }),
  "100%",
);

// The words: the engine's states, in the window's language.
assert.equal(statusLabel("running"), "Receiving data…");
assert.equal(statusLabel("cancelled"), "Stopped");
assert.equal(connectionLabel("receiving"), "Receiving data…");
assert.equal(connectionLabel("stopped"), "Stopped");
assert.equal(connectionLabel("done"), "Complete");
assert.equal(resumeLabel(null), "Not known yet");
assert.equal(resumeLabel(true), "Yes");
assert.equal(resumeLabel(false), "No");
assert.equal(segmentSize({ size: null }), "Unknown");
assert.equal(segmentSize({ size: 2048 }), "2.00 KB");

// The controls follow the state: a finished download offers its file and no
// command that could not act, a stopped one has no second stop, and a server
// that cannot resume from a range is honest about restarting.
const runningControls = progressControls({ status: "running", resumable: true });
assert.equal(runningControls.primary, "pause");
assert.equal(runningControls.canPause, true);
assert.equal(runningControls.canCancel, true);
assert.equal(runningControls.canResume, false);

const queuedControls = progressControls({ status: "queued", resumable: null });
assert.equal(queuedControls.canPause, true);

const pausedControls = progressControls({ status: "paused", resumable: true });
assert.equal(pausedControls.primary, "resume");
assert.equal(pausedControls.primaryLabel, "Resume");
assert.equal(pausedControls.canResume, true);
assert.equal(pausedControls.canCancel, true);

const restartControls = progressControls({ status: "failed", resumable: false });
assert.equal(restartControls.primaryLabel, "Restart");
assert.ok(
  restartControls.primaryHint.includes("first byte"),
  "a server that ignores ranges is a restart, not a resume",
);

// A paused transfer can still be open, so its control is a resume even when
// the file could not be resumed after it was stopped.
const pausedNoRanges = progressControls({ status: "paused", resumable: false });
assert.equal(pausedNoRanges.primaryLabel, "Resume");
assert.ok(pausedNoRanges.primaryHint.includes("still open"));

const stoppedControls = progressControls({ status: "cancelled", resumable: true });
assert.equal(stoppedControls.canCancel, false);
assert.equal(stoppedControls.cancelHint, "This download is already stopped.");

const completedControls = progressControls({
  status: "completed",
  resumable: true,
});
assert.equal(completedControls.primary, "open");
assert.equal(completedControls.primaryLabel, "Open file");
assert.equal(completedControls.canOpen, true);
assert.equal(completedControls.canClose, true);
assert.equal(completedControls.canPause, false);
assert.equal(completedControls.canResume, false);
assert.equal(completedControls.canCancel, false);

// The connections summary counts what is actually receiving.
assert.equal(
  receivingCount([
    { status: "receiving" },
    { status: "done" },
    { status: "receiving" },
  ]),
  2,
);

// The summary over the table names the state the connections are actually in.
assert.equal(connectionSummary([]), "Waiting for the plan");
assert.equal(
  connectionSummary([{ status: "done" }, { status: "done" }]),
  "2 complete",
);
assert.equal(
  connectionSummary([{ status: "receiving" }, { status: "done" }]),
  "1 receiving · 2 total",
);
assert.equal(
  connectionSummary([{ status: "failed" }, { status: "pending" }]),
  "1 failed · 2 total",
);
assert.equal(
  connectionSummary([{ status: "stopped" }, { status: "done" }]),
  "1 stopped · 2 total",
);
assert.equal(
  connectionSummary([{ status: "paused" }, { status: "paused" }]),
  "2 paused · 2 total",
);
assert.equal(
  connectionSummary([{ status: "retrying" }, { status: "pending" }]),
  "1 retrying · 2 total",
);
assert.equal(
  connectionSummary([{ status: "connecting" }, { status: "pending" }]),
  "1 connecting · 2 total",
);
assert.equal(
  connectionSummary([{ status: "pending" }, { status: "pending" }]),
  "2 waiting",
);

// The meter draws one block per connection, laid out along the file. A plan it
// cannot place (no ranges, an unknown file size, a range without a length) is
// a null, and the meter keeps its single bar.
const plan = [
  { id: 7, start: 0, size: 512, downloaded: 512, status: "done" },
  { id: 8, start: 512, size: 1024, downloaded: 256, status: "receiving" },
];
assert.deepEqual(progressChunks(plan, 4096), [
  { id: 7, left: 0, width: 12.5, fill: 100, status: "done" },
  { id: 8, left: 12.5, width: 25, fill: 25, status: "receiving" },
]);
assert.equal(progressChunks([], 4096), null, "no plan, no blocks");
assert.equal(progressChunks(plan, 0), null, "an unknown file size has no layout");
assert.equal(
  progressChunks(
    [{ id: 1, start: 0, size: null, downloaded: 10, status: "receiving" }],
    4096,
  ),
  null,
  "a range without a length cannot be placed",
);
assert.equal(
  progressChunks(
    [{ id: 1, start: 0, size: 100, downloaded: 0, status: "done" }],
    4096,
  )[0].fill,
  100,
  "a finished connection is full even before its bytes are counted",
);

// The rate: the first report has no delta, the second splits 65/35 between the
// old rate and the new one, and a paused download has no rate at all.
assert.equal(nextSpeedSample(undefined, 0, true, 1000).speed, 0);
const first = nextSpeedSample(
  { downloaded: 0, at: 1000, speed: 0, running: true },
  2048,
  true,
  2000,
);
assert.equal(first.speed, 2048, "the first delta is the first rate");
const second = nextSpeedSample(first, 2048, true, 3000);
assert.equal(
  Math.round(second.speed),
  Math.round(2048 * 0.65),
  "a report with no new bytes lets the rate fall",
);
const paused = nextSpeedSample(first, 2048, false, 3000);
assert.equal(paused.speed, 0, "a download that is not running has no rate");

// A pause is not a slow transfer: the first report after a resume is a fresh
// baseline, so the time spent stopped is not counted as time spent receiving.
const resumed = nextSpeedSample(paused, 4096, true, 60_000);
assert.equal(resumed.speed, 0, "the report after a resume is a baseline");
const afterResume = nextSpeedSample(resumed, 5120, true, 61_000);
assert.equal(afterResume.speed, 1024, "the rate counts from the resume");

// A report whose byte count went backwards belongs to a restarted download.
assert.equal(
  nextSpeedSample(first, 0, true, 3000).speed,
  0,
  "a restarted download starts a new baseline",
);

// Time left: only when both the size of what is left and the rate are known.
assert.equal(etaSeconds(8 * MB, 3 * MB, MB), 5);
assert.equal(etaSeconds(8 * MB, 3 * MB, 0), 0);
assert.equal(etaSeconds(0, 3 * MB, MB), 0);

// Speed limiter: the number the user types is bytes, and back again.
assert.equal(limitToBytes(256, "KB/s"), 256 * KB);
assert.equal(limitToBytes(2, "MB/s"), 2 * MB);
assert.deepEqual(bytesToLimit(256 * KB), { amount: 256, unit: "KB/s" });
assert.deepEqual(bytesToLimit(2 * MB), { amount: 2, unit: "MB/s" });
assert.equal(maxLimitAmount("KB/s"), 1024 * 1024);
assert.equal(maxLimitAmount("MB/s"), 1024);
assert.equal(parseLimitAmount("256", "KB/s"), 256);
assert.equal(parseLimitAmount("0", "KB/s"), null, "a cap of zero is a switch, not an amount");
assert.equal(parseLimitAmount("2.5", "MB/s"), null, "a fraction is not an amount");
assert.equal(parseLimitAmount("2048", "MB/s"), null, "over the ceiling");
assert.equal(parseLimitAmount("abc", "KB/s"), null);
assert.equal(limitLabel(0), "No limit");
assert.equal(limitLabel(512 * KB), "512 KB/s");

console.log("progress window: 79 checks pass");
