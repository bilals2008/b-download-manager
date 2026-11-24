import { describe, expect, it } from "vitest";

import {
  MAX_BATCH,
  batchPatternFromText,
  emptyBatchPattern,
  expandBatch,
  type BatchPattern,
} from "./batch";

function pattern(overrides: Partial<BatchPattern>): BatchPattern {
  return { ...emptyBatchPattern(), ...overrides };
}

describe("expandBatch", () => {
  it("expands a number range in order", () => {
    const result = expandBatch(
      pattern({
        address: "https://example.org/poster_*.jpg",
        from: "1",
        to: "3",
        padding: "1",
      }),
    );
    expect(result.problem).toBeNull();
    expect(result.urls).toEqual([
      "https://example.org/poster_1.jpg",
      "https://example.org/poster_2.jpg",
      "https://example.org/poster_3.jpg",
    ]);
  });

  it("keeps the width the reader asked for", () => {
    const result = expandBatch(
      pattern({
        address: "https://example.org/img_*.jpg",
        from: "7",
        to: "8",
        padding: "3",
      }),
    );
    expect(result.urls).toEqual([
      "https://example.org/img_007.jpg",
      "https://example.org/img_008.jpg",
    ]);
  });

  it("writes letters over a range", () => {
    const result = expandBatch(
      pattern({
        address: "https://example.org/part_*.zip",
        letters: true,
        from: "a",
        to: "c",
      }),
    );
    expect(result.problem).toBeNull();
    expect(result.urls).toEqual([
      "https://example.org/part_a.zip",
      "https://example.org/part_b.zip",
      "https://example.org/part_c.zip",
    ]);
  });

  it("says what is missing rather than guessing", () => {
    expect(expandBatch(emptyBatchPattern()).problem).toMatch(/Put the address in/);
    expect(
      expandBatch(pattern({ address: "https://example.org/a.jpg" })).problem,
    ).toMatch(/Put a \*/);
    expect(
      expandBatch(pattern({ address: "https://example.org/*_*.jpg" })).problem,
    ).toMatch(/one \*/);
  });

  it("refuses a range that runs backwards", () => {
    const result = expandBatch(
      pattern({ address: "https://example.org/*.jpg", from: "9", to: "2" }),
    );
    expect(result.urls).toEqual([]);
    expect(result.problem).toMatch(/or after it/);
  });

  it("stops at the size of a batch", () => {
    const result = expandBatch(
      pattern({
        address: "https://example.org/*.jpg",
        from: "1",
        to: String(MAX_BATCH + 1),
      }),
    );
    expect(result.urls).toEqual([]);
    expect(result.problem).toMatch(/stops at 200/);
  });

  it("refuses an address the engine would refuse", () => {
    const result = expandBatch(
      pattern({ address: "example.org/*.jpg", from: "1", to: "2" }),
    );
    expect(result.problem).toMatch(/full address/);
    // The addresses are still there, so the reader sees what they typed.
    expect(result.urls).toHaveLength(2);
  });

  it("does not mix the case of two letters", () => {
    const result = expandBatch(
      pattern({
        address: "https://example.org/*",
        letters: true,
        from: "a",
        to: "C",
      }),
    );
    expect(result.urls).toEqual([]);
    expect(result.problem).toMatch(/one case/);
  });
});

describe("batchPatternFromText", () => {
  it("turns the number a copied address ends in into the pattern", () => {
    const result = batchPatternFromText(
      "  https://example.org/show/episode_07.mkv  ",
    );
    expect(result).toEqual({
      address: "https://example.org/show/episode_*.mkv",
      letters: false,
      from: "7",
      to: "7",
      padding: "2",
    });
  });

  it("takes the last number of the address, not the first", () => {
    const result = batchPatternFromText("https://example.org/s2/ep_12.zip");
    expect(result?.address).toBe("https://example.org/s2/ep_*.zip");
    expect(result?.from).toBe("12");
  });

  it("gives nothing for a clipboard with no address, or no number", () => {
    expect(batchPatternFromText("just a note")).toBeNull();
    expect(batchPatternFromText("https://example.org/poster.jpg")).toBeNull();
  });
});
