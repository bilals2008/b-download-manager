//! When the queue is allowed to run.
//!
//! The rule lives in the engine because it must answer the same question in
//! the application and in a browser hand-over, with no window open: is now
//! inside a window? The answer is a pure function of the wall clock, so it
//! can be tested at a fixed time of day and it survives the machine being
//! asleep when a window opened.

use serde::{Deserialize, Serialize};

/// The days of the week, as the reader names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// Every day, in order, starting at Monday.
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    /// The day before this one, wrapping round the week.
    pub fn previous(self) -> Weekday {
        let index = Weekday::ALL.iter().position(|day| *day == self).unwrap_or(0);
        Weekday::ALL[(index + 6) % 7]
    }

    /// The day after this one, wrapping round the week.
    pub fn next(self) -> Weekday {
        let index = Weekday::ALL.iter().position(|day| *day == self).unwrap_or(0);
        Weekday::ALL[(index + 1) % 7]
    }

    /// The name a reader sees.
    pub fn label(self) -> &'static str {
        match self {
            Weekday::Mon => "Monday",
            Weekday::Tue => "Tuesday",
            Weekday::Wed => "Wednesday",
            Weekday::Thu => "Thursday",
            Weekday::Fri => "Friday",
            Weekday::Sat => "Saturday",
            Weekday::Sun => "Sunday",
        }
    }
}

/// What the queue does when the window closes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndAction {
    /// The queue keeps running until the reader stops it.
    #[default]
    KeepRunning,
    /// The queue stops and the downloads it started are paused.
    PauseQueue,
    /// The computer is asked to power off, after every download is saved.
    ShutDown,
}



/// The wall clock of the machine, as the schedule sees it.
///
/// The time is local: the reader thinks in the clock on the wall, not in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub weekday: Weekday,
    /// Minutes since midnight.
    pub minute: u16,
}

impl LocalTime {
    /// A time on a day, for the tests and for a schedule that is read back.
    pub fn at(weekday: Weekday, hour: u32, minute: u32) -> Self {
        Self {
            weekday,
            minute: (hour * 60 + minute).min(24 * 60 - 1) as u16,
        }
    }

    /// The time of day as `HH:MM`.
    pub fn clock(self) -> String {
        format!("{:02}:{:02}", self.minute / 60, self.minute % 60)
    }
}

/// When the next window opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextWindow {
    /// The day it opens on.
    pub weekday: Weekday,
    /// The minute of that day it opens at.
    pub minute: u16,
    /// How many days from now it is. Zero means today.
    pub in_days: u8,
}

impl NextWindow {
    /// The window as the status bar shows it: "tomorrow at 01:00".
    pub fn describe(&self) -> String {
        let when = match self.in_days {
            0 => "today".to_string(),
            1 => "tomorrow".to_string(),
            _ => format!("in {} days", self.in_days),
        };
        format!("{} at {}", when, format_minute(self.minute))
    }
}

/// The rule that says when the queue runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    /// Whether the schedule is in force. Off by default.
    #[serde(default)]
    pub enabled: bool,
    /// The days the window opens on. An empty list means the schedule is off,
    /// whatever `enabled` says.
    #[serde(default)]
    pub days: Vec<Weekday>,
    /// The minute after midnight the window opens.
    #[serde(default = "default_start")]
    pub start_minute: u16,
    /// The minute after midnight the window closes. When it is not after
    /// `start_minute` the window spans midnight.
    #[serde(default = "default_stop")]
    pub stop_minute: u16,
    /// What happens when the window closes.
    #[serde(default)]
    pub end_action: EndAction,
}

fn default_start() -> u16 {
    60
}

fn default_stop() -> u16 {
    6 * 60
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            days: Vec::new(),
            start_minute: default_start(),
            stop_minute: default_stop(),
            end_action: EndAction::KeepRunning,
        }
    }
}

impl Schedule {
    /// A schedule that never runs.
    pub fn is_empty(&self) -> bool {
        !self.enabled || self.days.is_empty()
    }

    /// Whether the window spans midnight.
    pub fn spans_midnight(&self) -> bool {
        self.stop_minute <= self.start_minute
    }

    /// Whether the given moment is inside a window.
    ///
    /// A window that spans midnight belongs to the day it opened on: a window
    /// from Monday 23:00 to Tuesday 01:00 is still open on Tuesday at 00:30,
    /// because Monday was one of its days.
    pub fn is_open_at(&self, now: LocalTime) -> bool {
        if self.is_empty() {
            return false;
        }
        if self.spans_midnight() {
            let opened_today = self.days.contains(&now.weekday) && now.minute >= self.start_minute;
            let opened_yesterday =
                self.days.contains(&now.weekday.previous()) && now.minute < self.stop_minute;
            opened_today || opened_yesterday
        } else {
            self.days.contains(&now.weekday)
                && now.minute >= self.start_minute
                && now.minute < self.stop_minute
        }
    }

    /// When the next window opens.
    ///
    /// `None` when the schedule never opens again. The search runs over the
    /// coming week, so a schedule on one day is found from any other day.
    pub fn next_open_at(&self, now: LocalTime) -> Option<NextWindow> {
        if self.is_empty() {
            return None;
        }
        let mut day = now.weekday;
        for step in 0..=7u8 {
            if self.days.contains(&day) && !(step == 0 && now.minute >= self.start_minute) {
                return Some(NextWindow {
                    weekday: day,
                    minute: self.start_minute,
                    in_days: step,
                });
            }
            day = day.next();
        }
        None
    }

    /// The moment the window that is open now closes: the day and the minute.
    pub fn closes_at(&self, now: LocalTime) -> Option<(Weekday, u16)> {
        if !self.is_open_at(now) {
            return None;
        }
        if self.spans_midnight() {
            let opened_yesterday = now.minute < self.stop_minute;
            let day = if opened_yesterday {
                now.weekday
            } else {
                now.weekday.next()
            };
            Some((day, self.stop_minute))
        } else {
            Some((now.weekday, self.stop_minute))
        }
    }

    /// A copy with the values a reader could have typed kept in range.
    pub fn sanitized(mut self) -> Self {
        self.start_minute = self.start_minute.min(24 * 60 - 1);
        self.stop_minute = self.stop_minute.min(24 * 60 - 1);
        let mut days: Vec<Weekday> = Vec::new();
        for day in self.days {
            if !days.contains(&day) {
                days.push(day);
            }
        }
        days.sort_by_key(|day| Weekday::ALL.iter().position(|x| x == day).unwrap_or(0));
        self.days = days;
        if self.days.is_empty() {
            self.enabled = false;
        }
        self
    }

    /// The schedule as the window shows it.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "the queue has no schedule".to_string();
        }
        let days: Vec<&str> = self.days.iter().map(|day| day.label()).collect();
        format!(
            "the queue runs {} to {} on {}",
            format_minute(self.start_minute),
            format_minute(self.stop_minute),
            days.join(", ")
        )
    }
}

/// A minute of the day as `HH:MM`.
pub fn format_minute(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// The wall clock of this machine, right now.
pub fn now_local() -> LocalTime {
    let now = chrono::Local::now();
    use chrono::{Datelike, Timelike};
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => Weekday::Mon,
        chrono::Weekday::Tue => Weekday::Tue,
        chrono::Weekday::Wed => Weekday::Wed,
        chrono::Weekday::Thu => Weekday::Thu,
        chrono::Weekday::Fri => Weekday::Fri,
        chrono::Weekday::Sat => Weekday::Sat,
        chrono::Weekday::Sun => Weekday::Sun,
    };
    LocalTime {
        weekday,
        minute: (now.hour() * 60 + now.minute()) as u16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overnight(days: Vec<Weekday>) -> Schedule {
        Schedule {
            enabled: true,
            days,
            start_minute: 23 * 60,
            stop_minute: 60,
            end_action: EndAction::KeepRunning,
        }
    }

    fn daytime(days: Vec<Weekday>) -> Schedule {
        Schedule {
            enabled: true,
            days,
            start_minute: 9 * 60,
            stop_minute: 17 * 60,
            end_action: EndAction::KeepRunning,
        }
    }

    #[test]
    fn a_schedule_that_is_off_never_opens() {
        let schedule = Schedule::default();
        assert!(schedule.is_empty());
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Mon, 1, 0)));
        assert_eq!(schedule.next_open_at(LocalTime::at(Weekday::Mon, 1, 0)), None);
    }

    #[test]
    fn a_window_inside_one_day_opens_and_closes() {
        let schedule = daytime(vec![Weekday::Mon, Weekday::Wed]);
        assert!(schedule.is_open_at(LocalTime::at(Weekday::Mon, 9, 0)));
        assert!(schedule.is_open_at(LocalTime::at(Weekday::Mon, 16, 59)));
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Mon, 17, 0)));
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Mon, 8, 59)));
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Tue, 12, 0)));
    }

    #[test]
    fn a_window_that_spans_midnight_belongs_to_the_day_it_opened_on() {
        let schedule = overnight(vec![Weekday::Mon]);
        assert!(schedule.is_open_at(LocalTime::at(Weekday::Mon, 23, 0)));
        assert!(schedule.is_open_at(LocalTime::at(Weekday::Mon, 23, 59)));
        assert!(schedule.is_open_at(LocalTime::at(Weekday::Tue, 0, 0)));
        assert!(schedule.is_open_at(LocalTime::at(Weekday::Tue, 0, 59)));
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Tue, 1, 0)));
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Mon, 22, 59)));
    }

    #[test]
    fn the_next_window_is_found_from_any_day() {
        let schedule = daytime(vec![Weekday::Wed]);
        let next = schedule
            .next_open_at(LocalTime::at(Weekday::Mon, 10, 0))
            .expect("a window on Wednesday");
        assert_eq!(next.weekday, Weekday::Wed);
        assert_eq!(next.minute, 9 * 60);
        assert_eq!(next.in_days, 2);

        let next = schedule
            .next_open_at(LocalTime::at(Weekday::Wed, 8, 0))
            .expect("today's window");
        assert_eq!(next.in_days, 0);

        let next = schedule
            .next_open_at(LocalTime::at(Weekday::Wed, 10, 0))
            .expect("next week's window");
        assert_eq!(next.weekday, Weekday::Wed);
        assert_eq!(next.in_days, 7);
    }

    #[test]
    fn the_next_window_is_described_for_the_status_bar() {
        let schedule = daytime(vec![Weekday::Wed]);
        let next = schedule
            .next_open_at(LocalTime::at(Weekday::Mon, 10, 0))
            .expect("a window");
        assert_eq!(next.describe(), "in 2 days at 09:00");
    }

    #[test]
    fn the_window_closes_on_the_day_it_opened_when_it_spans_midnight() {
        let schedule = overnight(vec![Weekday::Mon]);
        assert_eq!(
            schedule.closes_at(LocalTime::at(Weekday::Tue, 0, 30)),
            Some((Weekday::Tue, 60))
        );
        assert_eq!(
            schedule.closes_at(LocalTime::at(Weekday::Mon, 23, 30)),
            Some((Weekday::Tue, 60))
        );
        assert_eq!(schedule.closes_at(LocalTime::at(Weekday::Wed, 12, 0)), None);
    }

    #[test]
    fn a_missed_window_does_not_fire_later() {
        // The machine was asleep at 01:00 and woke at noon. Noon is not inside
        // the window, so nothing starts.
        let schedule = overnight(vec![Weekday::Mon]);
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Mon, 12, 0)));
    }

    #[test]
    fn a_schedule_with_no_days_is_off() {
        let schedule = Schedule {
            enabled: true,
            days: Vec::new(),
            ..Schedule::default()
        };
        assert!(schedule.is_empty());
        assert!(!schedule.is_open_at(LocalTime::at(Weekday::Mon, 12, 0)));
    }

    #[test]
    fn the_sanitiser_keeps_the_values_in_range() {
        let schedule = Schedule {
            enabled: true,
            days: vec![Weekday::Wed, Weekday::Mon, Weekday::Wed],
            start_minute: 65_535,
            stop_minute: 65_535,
            end_action: EndAction::PauseQueue,
        }
        .sanitized();
        assert_eq!(schedule.days, vec![Weekday::Mon, Weekday::Wed]);
        assert_eq!(schedule.start_minute, 24 * 60 - 1);
        assert_eq!(schedule.stop_minute, 24 * 60 - 1);
    }

    #[test]
    fn the_summary_reads_as_a_sentence() {
        let schedule = overnight(vec![Weekday::Sat, Weekday::Sun]);
        let text = schedule.summary();
        assert!(text.contains("23:00"), "{text}");
        assert!(text.contains("01:00"), "{text}");
        assert!(text.contains("Saturday"), "{text}");
        assert_eq!(Schedule::default().summary(), "the queue has no schedule");
    }

    #[test]
    fn a_minute_is_formatted_as_a_clock() {
        assert_eq!(format_minute(0), "00:00");
        assert_eq!(format_minute(9 * 60 + 5), "09:05");
        assert_eq!(format_minute(23 * 60 + 59), "23:59");
    }

    #[test]
    fn the_wall_clock_agrees_with_itself() {
        let now = now_local();
        assert!(now.minute < 24 * 60);
    }
}
