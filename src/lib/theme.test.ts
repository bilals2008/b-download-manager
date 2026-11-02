import { afterEach, describe, expect, it } from "vitest";

import { applyTheme, isLightTheme, resolveTheme } from "@/lib/theme";

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

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("resolveTheme", () => {
  it("returns a fixed preference as it is", () => {
    expect(resolveTheme("light")).toBe("light");
    expect(resolveTheme("dark")).toBe("dark");
    expect(resolveTheme("cyberpunk")).toBe("cyberpunk");
    expect(resolveTheme("volt")).toBe("volt");
    expect(resolveTheme("softblack")).toBe("softblack");
  });

  it("follows the operating system in auto mode", () => {
    preferLight(true);
    expect(resolveTheme("auto")).toBe("light");

    preferLight(false);
    expect(resolveTheme("auto")).toBe("dark");
  });
});

describe("isLightTheme", () => {
  it("names the appearances drawn on pale surfaces", () => {
    expect(isLightTheme("light")).toBe(true);
  });

  it("keeps the dark appearances off the light tray inks", () => {
    expect(isLightTheme("dark")).toBe(false);
    expect(isLightTheme("cyberpunk")).toBe(false);
    expect(isLightTheme("volt")).toBe(false);
    expect(isLightTheme("softblack")).toBe(false);
  });
});

describe("applyTheme", () => {
  it("writes the resolved appearance where the tokens look for it", () => {
    preferLight(true);
    applyTheme("auto");
    expect(document.documentElement.dataset.theme).toBe("light");

    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
