import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DownloadInfo } from "@/types";

const handlers = new Map<string, Set<(event: { payload: unknown }) => void>>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (event: { payload: unknown }) => void) => {
    let set = handlers.get(event);
    if (!set) {
      set = new Set();
      handlers.set(event, set);
    }
    set.add(handler);
    return Promise.resolve(() => {
      set.delete(handler);
    });
  },
}));

vi.mock("@/lib/errors", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/errors")>()),
  isDesktop: () => true,
}));

const engineRow: DownloadInfo = {
  id: 9,
  url: "https://example.org/setup.exe",
  dest: "D:\\BDM\\Programs\\setup.exe",
  total: 1000,
  downloaded: 1000,
  connections: 8,
  status: "completed",
  error: null,
  referer: null,
  segments: [],
  resumable: true,
  speedLimit: 0,
  category: "Programs",
  description: null,
};

const listDownloads = vi.fn(async () => [engineRow]);

vi.mock("@/lib/ipc", () => ({
  listDownloads: () => listDownloads(),
  onDownloadUpdate: () => Promise.resolve(() => {}),
  cancelDownload: () => Promise.resolve(),
  pauseDownload: () => Promise.resolve(),
  resumeDownload: () => Promise.resolve(),
  openPath: () => Promise.resolve(),
  setProgressDetails: () => Promise.resolve(),
  setDownloadSpeedLimit: () => Promise.resolve(),
}));

vi.mock("@/lib/media", () => ({
  mediaJob: () =>
    Promise.resolve({
      job: 5,
      state: "ready",
      error: null,
      notice: null,
      pageUrl: "https://example.org/watch",
      title: "A video",
      duration: 10,
      formats: [],
      pairs: [],
      subtitles: [],
      preferred: null,
      cookiesOffered: false,
      cookieConsent: false,
      downloading: true,
      done: false,
      dest: "D:\\BDM\\Video\\a video.mp4",
      stage: "Video",
      downloaded: 10,
      total: 100,
      segmentsDone: 1,
      segmentsTotal: 10,
      fraction: 0.1,
      canResume: false,
      resumeVideo: null,
      resumeAudio: null,
    }),
  mediaCancel: () => Promise.resolve(),
  mediaForget: () => Promise.resolve(),
  mediaResume: () => Promise.resolve(),
  mediaRedownload: () => Promise.resolve(),
}));

import { DownloadProgressWindow } from "./download-progress-window";

function fire(event: string, payload: unknown) {
  act(() => {
    for (const handler of handlers.get(event) ?? []) {
      handler({ payload });
    }
  });
}

describe("the one progress window", () => {
  beforeEach(() => {
    handlers.clear();
    listDownloads.mockClear();
  });

  it("shows the engine download a media-opened window is handed", async () => {
    // Opened for a video: no engine seed of its own.
    render(<DownloadProgressWindow mediaJob={5} />);
    await screen.findByText("A video");

    // An engine download takes the window over.
    fire("progress://job", { job: 9 });
    await waitFor(() => expect(screen.getByText("setup.exe")).toBeTruthy());
    expect(screen.getByText("Download status")).toBeTruthy();
  });

  it("moves back to a video and then to another engine download", async () => {
    render(<DownloadProgressWindow initial={engineRow} />);
    await screen.findByText("setup.exe");

    fire("progress://media", { job: 5 });
    await screen.findByText("A video");

    fire("progress://job", { job: 9 });
    await waitFor(() => expect(screen.getByText("setup.exe")).toBeTruthy());
  });

  it("reads the new download from the list when the seed is another file", async () => {
    const other = { ...engineRow, id: 4, dest: "D:\\BDM\\old.bin" };
    listDownloads.mockResolvedValueOnce([other, engineRow]);
    render(<DownloadProgressWindow initial={other} />);
    await screen.findByText("old.bin");

    fire("progress://job", { job: 9 });
    await waitFor(() => expect(screen.getByText("setup.exe")).toBeTruthy());
  });
});