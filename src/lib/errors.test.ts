import { afterEach, describe, expect, it } from "vitest";

import { describeError, isDesktop } from "@/lib/errors";

function becomeDesktop() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
}

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("isDesktop", () => {
  it("is false in a plain browser tab", () => {
    expect(isDesktop()).toBe(false);
  });

  it("is true when the Tauri bridge is present", () => {
    becomeDesktop();
    expect(isDesktop()).toBe(true);
  });
});

describe("describeError", () => {
  it("points a browser tab at the desktop app", () => {
    expect(describeError(new Error("anything"))).toBe(
      "Downloading works only in the desktop app.",
    );
  });

  it("prints the reason the desktop app gave", () => {
    becomeDesktop();
    expect(describeError("Disk is full.")).toBe("Disk is full.");
    expect(describeError(new Error("The server refused."))).toBe(
      "The server refused.",
    );
    expect(describeError({ message: "File exists.", path: "/a" })).toBe(
      "File exists.",
    );
  });

  it("never throws on a reason that is not printable", () => {
    becomeDesktop();
    expect(describeError(null)).toBe("The app did not answer.");
    expect(describeError(42)).toBe("The app did not answer.");
    expect(describeError({ message: 42 })).toBe("The app did not answer.");
  });
});
