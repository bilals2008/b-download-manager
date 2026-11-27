/**
 * Batch download: one address with a `*` standing for a number or a letter.
 *
 * A gallery is usually a hundred addresses that differ in one place
 * (`poster_01.jpg`, `poster_02.jpg`, ...). Typing them is not the job, and
 * pasting them all is not the job either: the reader writes the address once,
 * says which numbers it runs through, and sees the list before anything
 * starts. The expansion is plain data, so the dialog, the preview and the
 * tests all read the same rule.
 */

import { urlProblem } from "./downloads.ts";

/** The most addresses one batch may hold. Past this it is not a batch. */
export const MAX_BATCH = 200;

/** How many addresses the dialog prints before it counts the rest. */
export const PREVIEW_LIMIT = 6;

export interface BatchPattern {
  /** The address, with exactly one `*` where the number or the letter goes. */
  address: string;
  /** Letters instead of numbers: `A` to `C` rather than `1` to `3`. */
  letters: boolean;
  /** The first number, or the first letter. Kept as typed. */
  from: string;
  /** The last number, or the last letter. Kept as typed. */
  to: string;
  /**
   * How wide a number is written: `3` makes `007`. Ignored for letters.
   * Kept as typed so the field can be cleared and retyped.
   */
  padding: string;
}

export interface BatchExpansion {
  /** The addresses the pattern stands for, in order. */
  urls: string[];
  /**
   * Why the batch cannot start, in a reader's sentence; null when it can.
   * The addresses are still returned when they are usable one by one, so the
   * preview never disappears over a mistyped range.
   */
  problem: string | null;
}

/** A batch that has not been written yet: numbers, 1 to 10, one digit. */
export function emptyBatchPattern(): BatchPattern {
  return { address: "", letters: false, from: "1", to: "10", padding: "1" };
}

/** The same pattern with the address of a copied link, or the pattern as it is. */
function seeded(text: string): BatchPattern | null {
  const url = /https?:\/\/[^\s"'<>]+/i.exec(text);
  if (!url) {
    return null;
  }
  // The number a copied address ends in is the one the reader wants to run
  // through: `episode_07.mkv` becomes `episode_*.mkv`, 7 to 7, two digits.
  const runs = [...url[0].matchAll(/\d+/g)];
  const last = runs[runs.length - 1];
  if (!last || last.index === undefined) {
    return null;
  }
  const digits = last[0];
  const address =
    url[0].slice(0, last.index) + "*" + url[0].slice(last.index + digits.length);
  return {
    address,
    letters: false,
    from: String(Number(digits)),
    to: String(Number(digits)),
    padding: String(Math.min(10, Math.max(1, digits.length))),
  };
}

/**
 * A pattern made from a copied address, or null when the clipboard holds no
 * address with a number in it. This is what "Add batch download from
 * clipboard" opens with; the reader still sees and edits it.
 */
export function batchPatternFromText(text: string): BatchPattern | null {
  return seeded(text.trim());
}

/** A whole number, or null when the text is not one. */
function whole(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) {
    return null;
  }
  const value = Number(trimmed);
  return Number.isSafeInteger(value) ? value : null;
}

/** One letter, or null. */
function letter(text: string): string | null {
  const trimmed = text.trim();
  return /^[A-Za-z]$/.test(trimmed) ? trimmed : null;
}

/**
 * The addresses a pattern stands for.
 *
 * Every reason a batch cannot run is a sentence the reader can act on: the
 * missing `*`, a second one, a range that runs backwards, a range longer than
 * a batch, or an address the engine would refuse anyway.
 */
export function expandBatch(pattern: BatchPattern): BatchExpansion {
  const address = pattern.address.trim();
  if (!address) {
    return {
      urls: [],
      problem: "Put the address in, with a * where the number or the letter goes.",
    };
  }
  const parts = address.split("*");
  if (parts.length === 1) {
    return {
      urls: [],
      problem: "Put a * in the address where the numbers or the letters go.",
    };
  }
  if (parts.length > 2) {
    return { urls: [], problem: "Use one * only." };
  }

  const urls: string[] = [];
  if (pattern.letters) {
    const from = letter(pattern.from);
    const to = letter(pattern.to);
    if (!from || !to) {
      return { urls: [], problem: "Type one letter in From and in To." };
    }
    const start = from.charCodeAt(0);
    const end = to.charCodeAt(0);
    // `a` to `C` is not a range: the case of both ends has to agree.
    if ((start < 91) !== (end < 91)) {
      return { urls: [], problem: "Use one case for both letters." };
    }
    if (end < start) {
      return {
        urls: [],
        problem: "The last letter has to be the same as the first one, or after it.",
      };
    }
    for (let code = start; code <= end; code += 1) {
      urls.push(`${parts[0]}${String.fromCharCode(code)}${parts[1]}`);
    }
  } else {
    const from = whole(pattern.from);
    const to = whole(pattern.to);
    if (from === null || to === null) {
      return { urls: [], problem: "Type a whole number in From and in To." };
    }
    if (to < from) {
      return {
        urls: [],
        problem: "The last number has to be the same as the first one, or after it.",
      };
    }
    const count = to - from + 1;
    if (count > MAX_BATCH) {
      return {
        urls: [],
        problem: `A batch stops at ${MAX_BATCH} addresses; that range is ${count}.`,
      };
    }
    const padding = Math.min(10, Math.max(1, whole(pattern.padding) ?? 1));
    for (let value = from; value <= to; value += 1) {
      urls.push(`${parts[0]}${String(value).padStart(padding, "0")}${parts[1]}`);
    }
  }

  const refused = urls.find((url) => urlProblem(url) !== null);
  return {
    urls,
    problem: refused ? urlProblem(refused) : null,
  };
}
