/**
 * When the queue is allowed to run.
 *
 * The engine owns the schedule: it is stored in the engine's settings file
 * and read on a clock, so a window that opens while nobody is looking still
 * starts the queue, and a browser hand-over obeys it with no window open.
 */

import { invoke } from "@/lib/ipc";

/** The days of the week, as the engine names them. */
export type Weekday = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

/** What the queue does when the window closes. */
export type EndAction = "keepRunning" | "pauseQueue" | "shutDown";

/** The rule that says when the queue runs. */
export interface QueueSchedule {
  /** Whether the schedule is in force. */
  enabled: boolean;
  /** The days the window opens on. */
  days: Weekday[];
  /** The minute after midnight the window opens. */
  startMinute: number;
  /** The minute after midnight the window closes. */
  stopMinute: number;
  /** What happens when the window closes. */
  endAction: EndAction;
}

/** What the engine knows about the schedule right now. */
export interface ScheduleStatus {
  /** The rule itself, as the engine has it. */
  schedule: QueueSchedule;
  /** Whether a schedule is in force at all. */
  enabled: boolean;
  /** Whether now is inside a window. */
  open: boolean;
  /** The rule in one sentence. */
  summary: string;
  /** When the next window opens, as a reader reads it. */
  next: string | null;
  /** What happens when the window closes. */
  endAction: EndAction;
  /** Whether the computer may be powered off when the window closes. */
  powerOff: boolean;
}

/** The schedule in force, as the engine has it. */
export function queueSchedule(): Promise<ScheduleStatus> {
  return invoke<ScheduleStatus>("queue_schedule");
}

/** Keep the schedule the reader chose, and the choice about powering off. */
export function setQueueSchedule(
  schedule: QueueSchedule,
  powerOff: boolean,
): Promise<ScheduleStatus> {
  return invoke<ScheduleStatus>("set_queue_schedule", { schedule, powerOff });
}
