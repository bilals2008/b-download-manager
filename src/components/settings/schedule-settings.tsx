import { useEffect, useState } from "react";

import { Section, SettingRow } from "@/components/settings/settings-page";
import { ListBox } from "@/components/ui/listbox";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import {
  queueSchedule,
  setQueueSchedule,
  type EndAction,
  type QueueSchedule,
  type Weekday,
} from "@/lib/ipc-schedule";

/** The days, in the order the week runs. */
const DAYS: { value: Weekday; label: string }[] = [
  { value: "mon", label: "Mon" },
  { value: "tue", label: "Tue" },
  { value: "wed", label: "Wed" },
  { value: "thu", label: "Thu" },
  { value: "fri", label: "Fri" },
  { value: "sat", label: "Sat" },
  { value: "sun", label: "Sun" },
];

/** What the queue does when the window closes. */
const END_ACTIONS: { value: EndAction; label: string }[] = [
  { value: "keepRunning", label: "Keep running" },
  { value: "pauseQueue", label: "Pause the queue" },
  { value: "shutDown", label: "Turn the computer off" },
];

/** The schedule as it stands before anything is read. */
const EMPTY: QueueSchedule = {
  enabled: false,
  days: [],
  startMinute: 60,
  stopMinute: 360,
  endAction: "keepRunning",
};

/** A minute of the day as the time field shows it. */
function asClock(minute: number): string {
  const hours = Math.floor(minute / 60) % 24;
  const minutes = minute % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

/** A time field's value as a minute of the day. */
function asMinute(value: string, fallback: number): number {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!match) {
    return fallback;
  }
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (hours > 23 || minutes > 59) {
    return fallback;
  }
  return hours * 60 + minutes;
}

/**
 * When the queue runs.
 *
 * The engine owns the rule: it is kept in the engine's settings file and read
 * on a clock, so a window that opens at one in the morning starts the queue
 * whether or not anybody is sitting in front of the machine. This page reads
 * that file and writes to it.
 *
 * The computer is never turned off because a schedule said so and nothing
 * else: powering off is a separate switch, and a download still running when
 * the window closes is refused rather than forced.
 */
export function ScheduleSettings() {
  const [schedule, setSchedule] = useState<QueueSchedule>(EMPTY);
  const [powerOff, setPowerOff] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    void queueSchedule()
      .then((status) => {
        if (!active) {
          return;
        }
        setSchedule(status.schedule);
        setPowerOff(status.powerOff);
      })
      .catch((failure: unknown) => {
        if (active) {
          setProblem(describeError(failure));
        }
      });
    return () => {
      active = false;
    };
  }, []);

  function keep(next: QueueSchedule, nextPowerOff = powerOff) {
    setSchedule(next);
    void setQueueSchedule(next, nextPowerOff)
      .then((status) => {
        setProblem(null);
        setPowerOff(status.powerOff);
      })
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      });
  }

  function toggleDay(day: Weekday) {
    const days = schedule.days.includes(day)
      ? schedule.days.filter((one) => one !== day)
      : [...schedule.days, day];
    keep({ ...schedule, days });
  }

  return (
    <>
      <Section title="Schedule">
        <SettingRow
          settingId="setting-queue-schedule"
          label="Run the queue on a schedule"
        >
          <Switch
            checked={schedule.enabled}
            label="Run the queue on a schedule"
            onChange={(on) => keep({ ...schedule, enabled: on })}
          />
        </SettingRow>
        <SettingRow label="Days">
          <div className="flex gap-1" role="group" aria-label="Days the queue runs">
            {DAYS.map((day) => {
              const on = schedule.days.includes(day.value);
              return (
                <button
                  key={day.value}
                  type="button"
                  aria-pressed={on}
                  onClick={() => toggleDay(day.value)}
                  className={
                    on
                      ? "h-7 w-9 rounded-chrome bg-primary text-xs text-primary-foreground"
                      : "h-7 w-9 rounded-chrome bg-chrome-menu-hover text-xs text-chrome-menu-text"
                  }
                >
                  {day.label}
                </button>
              );
            })}
          </div>
        </SettingRow>
        <SettingRow settingId="setting-queue-start" label="Start">
          <input
            id="setting-queue-start"
            type="time"
            value={asClock(schedule.startMinute)}
            onChange={(event) =>
              keep({
                ...schedule,
                startMinute: asMinute(event.target.value, schedule.startMinute),
              })
            }
            className="h-8 rounded-chrome border border-chrome-line bg-chrome-panel px-2 text-xs tabular-nums text-chrome-title-text"
          />
        </SettingRow>
        <SettingRow settingId="setting-queue-stop" label="Stop">
          <input
            id="setting-queue-stop"
            type="time"
            value={asClock(schedule.stopMinute)}
            onChange={(event) =>
              keep({
                ...schedule,
                stopMinute: asMinute(event.target.value, schedule.stopMinute),
              })
            }
            className="h-8 rounded-chrome border border-chrome-line bg-chrome-panel px-2 text-xs tabular-nums text-chrome-title-text"
          />
        </SettingRow>
        <SettingRow
          settingId="setting-queue-end"
          label="When the window closes"
        >
          <ListBox
            label="When the window closes"
            options={END_ACTIONS}
            value={schedule.endAction}
            onChange={(value) => keep({ ...schedule, endAction: value as EndAction })}
          />
        </SettingRow>
        <SettingRow label="Turn the computer off">
          <Switch
            checked={powerOff}
            label="Turn the computer off"
            disabled={schedule.endAction !== "shutDown"}
            onChange={(on) => {
              setPowerOff(on);
              keep(schedule, on);
            }}
          />
        </SettingRow>
        {problem ? (
          <p className="pt-2 text-xs text-chrome-danger">{problem}</p>
        ) : null}
      </Section>
    </>
  );
}
