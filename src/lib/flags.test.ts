import { describe, expect, it } from "vitest";

import { flagUrl } from "@/lib/flags";

describe("flagUrl", () => {
  it("finds a country's flag whatever case the code arrives in", () => {
    expect(flagUrl("IN")).toBeTruthy();
    expect(flagUrl("in")).toBe(flagUrl("IN"));
    expect(flagUrl(" de ")).toBeTruthy();
  });

  it("has no flag for nothing, for a word, or for a country it does not carry", () => {
    expect(flagUrl(null)).toBeNull();
    expect(flagUrl(undefined)).toBeNull();
    expect(flagUrl("")).toBeNull();
    expect(flagUrl("zz")).toBeNull();
    expect(flagUrl("india")).toBeNull();
  });
});
