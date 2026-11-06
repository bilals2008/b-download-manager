import { afterEach, describe, expect, it } from "vitest";

import {
  DEFAULT_SETTINGS,
  initialTheme,
  loadSettings,
  saveSettings,
} from "@/lib/settings";

function preferLight(light: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: light,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

describe("initialTheme", () => {
  it("starts in Ayu Dark on a dark system", () => {
    preferLight(false);
    expect(initialTheme()).toBe("ayu");
  });

  it("starts in Bloom when the system asks for a light appearance", () => {
    preferLight(true);
    expect(initialTheme()).toBe("bloom");
  });

  it("starts in Ayu Dark when the host cannot answer the question", () => {
    const matchMedia = window.matchMedia;
    // The self-check scripts hand `window` to this module themselves, and a
    // stub that leaves matchMedia out is the state they can be left in.
    delete (window as { matchMedia?: typeof window.matchMedia }).matchMedia;
    try {
      expect(initialTheme()).toBe("ayu");
    } finally {
      window.matchMedia = matchMedia;
    }
  });
});

describe("the first open", () => {
  afterEach(() => {
    window.localStorage.clear();
    preferLight(false);
  });

  it("reads the system appearance when nothing is saved", () => {
    window.localStorage.clear();
    preferLight(true);
    expect(loadSettings().theme).toBe("bloom");
    preferLight(false);
    expect(loadSettings().theme).toBe("ayu");
  });

  it("keeps the saved appearance whatever the system asks for", () => {
    saveSettings({ ...DEFAULT_SETTINGS, theme: "graphite" });
    preferLight(true);
    expect(loadSettings().theme).toBe("graphite");
  });
});
