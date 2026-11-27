import { describe, expect, it } from "vitest";

import { FLAG_ATLAS, flagCell } from "@/lib/flags";

describe("flagCell", () => {
  it("finds a country's cell whatever case the code arrives in", () => {
    // The atlas is written in code order, so the first country is its first cell.
    expect(flagCell("ad")).toEqual({ x: 0, y: 0 });
    expect(flagCell("ae")).toEqual({ x: FLAG_ATLAS.cellWidth, y: 0 });
    expect(flagCell("IN")).toEqual(flagCell("in"));
    expect(flagCell(" de ")).toEqual(flagCell("de"));
  });

  it("has no cell for nothing, for a word, or for a country it does not carry", () => {
    expect(flagCell(null)).toBeNull();
    expect(flagCell(undefined)).toBeNull();
    expect(flagCell("")).toBeNull();
    expect(flagCell("zz")).toBeNull();
    expect(flagCell("india")).toBeNull();
  });
});
