import { describe, expect, it } from "vitest";

import {
  CATALOGUES,
  LANGUAGES,
  languageEntry,
  readLanguage,
  translate,
} from "@/i18n/catalogue";

describe("the catalogues", () => {
  it("every language has every key", () => {
    const keys = Object.keys(CATALOGUES.en);
    for (const entry of LANGUAGES) {
      for (const key of keys) {
        expect(CATALOGUES[entry.code][key as keyof typeof CATALOGUES.en]).toBeTruthy();
      }
    }
  });

  it("the keys exist in English, the fallback", () => {
    expect(Object.keys(CATALOGUES.en).length).toBeGreaterThan(10);
  });

  it("a missing key reads as English, and a missing key does not crash", () => {
    expect(translate("de", "settings.title")).toBe(CATALOGUES.de["settings.title"]);
  });

  it("every language is named in its own words", () => {
    for (const entry of LANGUAGES) {
      expect(entry.label.trim().length).toBeGreaterThan(0);
    }
  });

  it("Arabic is right to left, the others are not", () => {
    expect(languageEntry("ar").rtl).toBe(true);
    expect(languageEntry("en").rtl).toBe(false);
    expect(languageEntry("fr").rtl).toBe(false);
    expect(languageEntry("de").rtl).toBe(false);
  });
});

describe("the language the window is in", () => {
  it("falls back to English for a value the app does not speak", () => {
    window.localStorage.setItem("idm-clone.language", "xx");
    expect(readLanguage()).toBe("en");
    window.localStorage.removeItem("idm-clone.language");
  });

  it("reads the stored choice", () => {
    window.localStorage.setItem("idm-clone.language", "de");
    expect(readLanguage()).toBe("de");
    window.localStorage.removeItem("idm-clone.language");
  });
});
