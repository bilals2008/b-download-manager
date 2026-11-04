import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  installUpdate: vi.fn(),
  restartApp: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/lib/updates", () => ({
  installUpdate: mocks.installUpdate,
  restartApp: mocks.restartApp,
  describeFailure: (failure: unknown) =>
    failure instanceof Error ? failure.message : String(failure),
}));

import {
  beginUpdate,
  clearUpdateRun,
  currentUpdateRun,
} from "@/state/update-store";

const FOUND = {
  available: true,
  currentVersion: "0.3.1",
  version: "0.4.0",
  date: null,
  notes: null,
  error: null,
};

/** Let the store's promise chain publish what the mocked command answered. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("update store", () => {
  beforeEach(() => {
    mocks.installUpdate.mockReset();
    mocks.restartApp.mockReset();
    clearUpdateRun();
  });

  it("follows the backend's stage and byte count", async () => {
    mocks.installUpdate.mockImplementation(
      (onProgress: (progress: unknown) => void) => {
        return Promise.resolve().then(() => {
          onProgress({ version: "0.4.0", stage: "downloading", downloaded: 10, total: 20 });
          onProgress({ version: "0.4.0", stage: "verifying", downloaded: 20, total: 20 });
          onProgress({ version: "0.4.0", stage: "installing", downloaded: 20, total: 20 });
          return { restartRequired: true };
        });
      },
    );

    beginUpdate(FOUND);
    expect(currentUpdateRun().phase).toBe("downloading");

    await settle();
    expect(currentUpdateRun().phase).toBe("restart-ready");
    expect(currentUpdateRun().downloaded).toBe(20);
    expect(currentUpdateRun().total).toBe(20);
  });

  it("shows the failure instead of claiming an install", async () => {
    mocks.installUpdate.mockRejectedValue(new Error("the signature did not match"));

    beginUpdate(FOUND);
    await settle();

    expect(currentUpdateRun().phase).toBe("error");
    expect(currentUpdateRun().error).toBe("the signature did not match");
  });

  it("clears a finished failure so the notice can leave", async () => {
    mocks.installUpdate.mockRejectedValue(new Error("offline"));

    beginUpdate(FOUND);
    await settle();
    clearUpdateRun();

    expect(currentUpdateRun().phase).toBe("idle");
    expect(currentUpdateRun().update).toBeNull();
  });
});
