import { describe, expect, it } from "vitest";

import {
  formatDuration,
  initialQuality,
  mediaRows,
  qualityToken,
  type MediaJobView,
} from "@/lib/media";

function job(overrides: Partial<MediaJobView> = {}): MediaJobView {
  return {
    job: 1,
    state: "ready",
    error: null,
    notice: null,
    pageUrl: "https://example.org/watch",
    title: "A video",
    duration: 62,
    formats: [
      {
        id: "v1080",
        kind: "video",
        container: "mp4",
        codec: "avc1",
        width: 1920,
        height: 1080,
        fps: 30,
        bandwidth: 5_000_000,
        size: 1_000,
        label: "1080p",
      },
      {
        id: "v720",
        kind: "video",
        container: "mp4",
        codec: "avc1",
        width: 1280,
        height: 720,
        fps: 30,
        bandwidth: 2_500_000,
        size: 800,
        label: "720p",
      },
      {
        id: "a1",
        kind: "audio",
        container: "m4a",
        codec: "mp4a",
        width: null,
        height: null,
        fps: null,
        bandwidth: 128_000,
        size: 128,
        label: "Audio",
      },
      {
        id: "a2",
        kind: "audio",
        container: "m4a",
        codec: "mp4a",
        width: null,
        height: null,
        fps: null,
        bandwidth: 128_000,
        size: 128,
        label: "Audio only",
      },
    ],
    pairs: [
      { video: "v1080", audio: "a1", label: "1080p" },
      { video: "v720", audio: null, label: "720p" },
    ],
    preferred: null,
    cookiesOffered: false,
    cookieConsent: false,
    downloading: false,
    done: false,
    dest: null,
    stage: null,
    downloaded: 0,
    total: 0,
    segmentsDone: 0,
    segmentsTotal: 0,
    ...overrides,
  };
}

describe("mediaRows", () => {
  it("adds the paired audio to the video's size and keeps a silent row marked", () => {
    const rows = mediaRows(job());
    const high = rows.find((row) => row.id === "v1080");
    const low = rows.find((row) => row.id === "v720");

    expect(high?.size).toBe(1_128);
    expect(high?.audio).toBe("a1");
    expect(high?.note).toBeNull();
    expect(low?.size).toBe(800);
    expect(low?.note).toBe("No sound");
  });

  it("orders the picture by height and puts audio-only tracks last", () => {
    const rows = mediaRows(job());
    expect(rows.map((row) => row.id)).toEqual(["v1080", "v720", "a2"]);
  });

  it("keeps a video format the pair list never mentioned", () => {
    const view = job();
    view.formats.push({
      ...view.formats[1],
      id: "v480",
      height: 480,
      label: "480p",
      size: 400,
    });

    const rows = mediaRows(view);
    expect(rows.map((row) => row.id)).toEqual(["v1080", "v720", "v480", "a2"]);
    expect(rows[2].note).toBe("No sound");
  });
});

describe("initialQuality", () => {
  it("picks 1080p when the host has no preference", () => {
    expect(initialQuality(job())).toBe("v1080");
  });

  it("honours a remembered quality word", () => {
    expect(initialQuality(job({ preferred: "720p" }))).toBe("v720");
  });

  it("falls back to the closest height the manifest offers", () => {
    expect(initialQuality(job({ preferred: "900p" }))).toBe("v720");
  });

  it("has no answer for an empty format list", () => {
    expect(initialQuality(job({ formats: [], pairs: [] }))).toBeNull();
  });
});

describe("qualityToken", () => {
  it("remembers the resolution when the manifest stated one", () => {
    expect(qualityToken(job(), "v720")).toBe("720p");
  });

  it("falls back to the label, then to nothing", () => {
    expect(qualityToken(job(), "a1")).toBe("Audio");
    expect(qualityToken(job(), "missing")).toBe("");
  });
});

describe("formatDuration", () => {
  it("prints minutes, and hours only when there are any", () => {
    expect(formatDuration(62)).toBe("1:02");
    expect(formatDuration(3661)).toBe("1:01:01");
  });

  it("rounds to the nearest second", () => {
    expect(formatDuration(59.6)).toBe("1:00");
  });

  it("prints nothing for a missing or impossible length", () => {
    expect(formatDuration(null)).toBe("");
    expect(formatDuration(0)).toBe("");
    expect(formatDuration(Number.NaN)).toBe("");
  });
});
