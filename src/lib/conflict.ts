import { splitPath } from "./downloads.ts";
import type {
  ConflictChoice,
  StartOutcome,
  StartRefusal,
} from "@/lib/ipc";

/** What the File Exists dialog can answer. Cancel is a dismissal, not one. */
export type ConflictAnswer = Exclude<ConflictChoice, "ask">;

/** The destination a start could not take, and what the dialog can offer. */
export interface FileConflict {
  /** The file the start asked for. */
  path: string;
  name: string;
  folder: string;
  /** A free name beside it, for the Rename answer. */
  suggestedName: string;
  /** Another download in the list is already writing this file. */
  active: boolean;
  /** Files after this one in a batch, which may ask the same question. */
  remaining: number;
}

/** The answer the window gave, and whether it covers the rest of a batch. */
export interface ConflictReply {
  choice: ConflictAnswer;
  applyToRemaining: boolean;
}

/** The user dismissed the question: nothing should be started. */
export class StartCancelled extends Error {
  constructor() {
    super("The destination question was dismissed.");
    this.name = "StartCancelled";
  }
}

/**
 * Turn a refusal of the engine into the question the window shows.
 *
 * The name and the folder come from the path the engine answered with, so the
 * window shows the file the user actually asked for, not one it guessed.
 */
export function conflictFrom(
  refusal: StartRefusal,
  remaining: number,
): FileConflict {
  const { dir, base } = splitPath(refusal.path);
  const suggested = splitPath(refusal.suggested).base;
  return {
    path: refusal.path,
    name: base || refusal.path,
    folder: dir,
    suggestedName: suggested && suggested !== base ? suggested : "",
    active: refusal.kind === "active",
    remaining: Math.max(0, remaining),
  };
}

/** Whether an answer can be offered for the rest of a batch too. */
export function canApplyToRemaining(conflict: FileConflict | null): boolean {
  return (conflict?.remaining ?? 0) > 0;
}

/** The checkbox beside the answers: one row of a batch, in its own words. */
export function applyToRemainingLabel(remaining: number): string {
  if (remaining <= 0) {
    return "";
  }
  return remaining === 1
    ? "Apply to the remaining file"
    : `Apply to the remaining ${remaining} files`;
}

/** Whether an answer may be picked for this conflict. */
export function answerAllowed(
  conflict: FileConflict,
  answer: ConflictAnswer,
): boolean {
  // A file another download holds cannot be replaced without that download
  // losing it, so the engine refuses that one answer.
  return !(conflict.active && answer === "replace");
}

/** Whether a rejected command is a destination conflict. */
export function isStartRefusal(error: unknown): error is StartRefusal {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "suggested" in error &&
    typeof (error as { kind: unknown }).kind === "string"
  );
}

/** The rest of a batch, while its files are started one after another. */
export interface BatchAsking {
  /** Files after the one that is asking. */
  remaining: number;
  /** The answer the user gave for the rest, once they have given one. */
  choice: ConflictAnswer | null;
}

/**
 * Start something, asking about a destination that is already taken.
 *
 * `attempt` is the command itself, so a new download, a redownload and a
 * hand-over all ask the same question the same way. A batch reuses one answer
 * for the files after the one that asked; an answer that still does not settle
 * a destination is reported instead of asking again forever.
 */
export async function startResolving(
  attempt: (choice: ConflictChoice) => Promise<StartOutcome>,
  ask: (conflict: FileConflict) => Promise<ConflictReply | null>,
  batch?: BatchAsking,
): Promise<StartOutcome> {
  let choice: ConflictChoice = batch?.choice ?? "ask";
  let answered = false;
  for (;;) {
    try {
      return await attempt(choice);
    } catch (failure) {
      if (!isStartRefusal(failure)) {
        throw failure;
      }
      if (failure.kind !== "exists" && failure.kind !== "active") {
        throw failure;
      }
      if (answered) {
        throw failure;
      }
      // The answer given to an earlier file was this file's first attempt
      // already. If it did not settle this destination, ask about this one.
      if (batch) {
        batch.choice = null;
      }
      const reply = await ask(conflictFrom(failure, batch?.remaining ?? 0));
      if (!reply) {
        throw new StartCancelled();
      }
      answered = true;
      if (reply.applyToRemaining && batch) {
        batch.choice = reply.choice;
      }
      choice = reply.choice;
    }
  }
}
