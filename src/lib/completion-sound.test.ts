import { describe, expect, it } from "vitest";

import {
  COMPLETION_SOUNDS,
  completionLevel,
  completionNotes,
  DEFAULT_COMPLETION_SOUND,
  isCompletionSound,
  playCompletionSound,
} from "@/lib/completion-sound";

describe("the completion sound", () => {
  it("is a few short, audible notes per sound", () => {
    for (const { value } of COMPLETION_SOUNDS) {
      const notes = completionNotes(value);
      expect(notes.length).toBeGreaterThan(0);
      for (const note of notes) {
        expect(note.frequency).toBeGreaterThan(100);
        expect(note.frequency).toBeLessThan(8000);
        expect(note.duration).toBeGreaterThan(0.02);
        expect(note.duration).toBeLessThan(1);
        expect(note.peak).toBeGreaterThan(0);
        expect(note.peak).toBeLessThan(0.5);
      }
    }
  });

  it("plays the default for a name it does not know", () => {
    expect(isCompletionSound("kazoo")).toBe(false);
    expect(completionNotes("kazoo")).toEqual(
      completionNotes(DEFAULT_COMPLETION_SOUND),
    );
    expect(completionNotes(undefined)).toEqual(
      completionNotes(DEFAULT_COMPLETION_SOUND),
    );
  });

  it("keeps the volume between silent and full", () => {
    expect(completionLevel(0)).toBe(0);
    expect(completionLevel(60)).toBeCloseTo(0.6);
    expect(completionLevel(400)).toBe(1);
    expect(completionLevel(-20)).toBe(0);
    expect(completionLevel("loud")).toBe(0);
    expect(completionLevel(undefined)).toBe(0);
  });

  it("stays silent at zero volume", () => {
    expect(playCompletionSound("chime", 0)).toBe(false);
  });

  it("reports a host it cannot play on instead of throwing", () => {
    // The check host has no audio device: playable is reported, never thrown.
    expect(playCompletionSound("chime", 60)).toBe(false);
    expect(() => playCompletionSound(undefined, undefined)).not.toThrow();
  });
});
