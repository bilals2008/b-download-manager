import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useBatchSeed } from "@/components/workspace/batch-seed";
import { emptyBatchPattern, type BatchPattern } from "@/lib/batch";

/** The address the clipboard answers with, a moment after the dialog opened. */
const fromClipboard: BatchPattern = {
  address: "https://example.org/episode_*.mkv",
  letters: false,
  from: "7",
  to: "7",
  padding: "2",
};

function probe(initialOpen = false) {
  return renderHook<
    ReturnType<typeof useBatchSeed>,
    { open: boolean; seed: BatchPattern | null }
  >(({ open, seed }) => useBatchSeed(open, seed), {
    initialProps: { open: initialOpen, seed: null },
  });
}

describe("useBatchSeed", () => {
  it("fills an untouched form when the clipboard answers late", () => {
    const { result, rerender } = probe();

    // "Add batch download from clipboard" opens the dialog first, then reads.
    act(() => rerender({ open: true, seed: null }));
    expect(result.current.pattern.address).toBe("");

    act(() => rerender({ open: true, seed: fromClipboard }));

    expect(result.current.pattern.address).toBe("https://example.org/episode_*.mkv");
    expect(result.current.pattern.from).toBe("7");
  });

  it("keeps what the reader typed when the clipboard answers late", () => {
    const { result, rerender } = probe();

    act(() => rerender({ open: true, seed: null }));
    act(() => result.current.change({ address: "https://mine.example/photo_*.jpg" }));
    act(() => rerender({ open: true, seed: fromClipboard }));

    expect(result.current.pattern.address).toBe("https://mine.example/photo_*.jpg");
  });

  it("keeps a touched range as well as a touched address", () => {
    const { result, rerender } = probe();

    act(() => rerender({ open: true, seed: null }));
    act(() => result.current.change({ to: "12" }));
    act(() => rerender({ open: true, seed: fromClipboard }));

    expect(result.current.pattern.to).toBe("12");
    expect(result.current.pattern.address).toBe("");
  });

  it("opens a fresh dialog on the seed it was given", () => {
    const { result, rerender } = probe();

    act(() => rerender({ open: true, seed: fromClipboard }));
    expect(result.current.pattern.address).toBe("https://example.org/episode_*.mkv");

    // Closed, then opened again from the menu: the old pattern is gone.
    act(() => rerender({ open: false, seed: null }));
    act(() => rerender({ open: true, seed: null }));

    expect(result.current.pattern).toEqual(emptyBatchPattern());
  });

  it("lets the reader write again after a seed was applied", () => {
    const { result, rerender } = probe();

    act(() => rerender({ open: true, seed: null }));
    act(() => rerender({ open: true, seed: fromClipboard }));
    act(() => result.current.change({ address: "https://edited.example/*.png" }));
    act(() => rerender({ open: true, seed: fromClipboard }));

    expect(result.current.pattern.address).toBe("https://edited.example/*.png");
  });
});
