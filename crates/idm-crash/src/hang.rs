//! The hang probe of the monitor.
//!
//! A heartbeat that stops says the application is not writing, which is what a
//! frozen window looks like — and also what a machine that went to sleep looks
//! like. The difference is whether the window still answers: a process that is
//! suspended and resumed answers at once, while a program whose main thread is
//! stuck does not answer at all.
//!
//! The probe is a window message with a short timeout. It is the cheapest
//! question the system can answer about another program's message loop, and it
//! is the same one the shell asks before it offers to end a program.

use std::time::Duration;

/// How long the application is given to answer before it is called hung.
pub const ANSWER_WITHIN: Duration = Duration::from_millis(300);

/// How long the application may be quiet before the monitor asks.
pub const FREEZE_AFTER: Duration = Duration::from_secs(20);

/// What the window said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The window answered inside the timeout.
    Answers,
    /// The window did not answer inside the timeout.
    Hung,
    /// The process has no window to ask.
    NoWindow,
}

/// What the monitor should report, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Episode {
    /// Nothing worth writing down.
    Quiet,
    /// A freeze that has not been written down yet.
    Frozen {
        /// How long the application had been quiet.
        quiet_for: Duration,
    },
    /// The application answered again after a freeze.
    Recovered,
}

/// The freeze episodes of one run.
///
/// One report per episode: a program that is stuck for a minute must not fill
/// the folder with a report a second, and a program that recovers and sticks
/// again is a second episode worth reporting.
#[derive(Debug, Default)]
pub struct Episodes {
    reported: bool,
}

impl Episodes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look at one beat and one answer, and say what should happen.
    ///
    /// `quiet_for` is how long the application has been quiet, or `None` when
    /// it has never beaten.
    pub fn observe(&mut self, quiet_for: Option<Duration>, answer: Answer) -> Episode {
        let quiet_for = match quiet_for {
            Some(quiet) if quiet > FREEZE_AFTER => quiet,
            _ => {
                // Beating again ends any episode there was.
                let recovered = self.reported;
                self.reported = false;
                return if recovered {
                    Episode::Recovered
                } else {
                    Episode::Quiet
                };
            }
        };

        match answer {
            // A quiet application whose window still answers was suspended,
            // or was busy and caught up: not a freeze.
            Answer::Answers => {
                let recovered = self.reported;
                self.reported = false;
                if recovered {
                    Episode::Recovered
                } else {
                    Episode::Quiet
                }
            }
            Answer::NoWindow => Episode::Quiet,
            Answer::Hung => {
                if self.reported {
                    Episode::Quiet
                } else {
                    self.reported = true;
                    Episode::Frozen { quiet_for }
                }
            }
        }
    }
}

/// Ask the process whether its window answers.
#[cfg(target_os = "windows")]
pub fn ask(pid: u32) -> Answer {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW, SMTO_ABORTIFHUNG,
        WM_NULL,
    };

    /// The message result is not read, only whether there was one.
    const SMTO_BLOCK: u32 = 0x0001;

    /// What `EnumWindows` is given: the process it is looking for, and the
    /// window it found.
    struct Search {
        pid: u32,
        window: HWND,
    }

    unsafe extern "system" fn visit(window: HWND, parameter: isize) -> i32 {
        let search = &mut *(parameter as *mut Search);
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut owner = 0u32;
        GetWindowThreadProcessId(window, &mut owner);
        if owner == search.pid {
            search.window = window;
            // The first visible window of the process is its main window.
            return 0;
        }
        1
    }

    let mut search = Search {
        pid,
        window: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as isize);
    }
    if search.window.is_null() {
        return Answer::NoWindow;
    }

    let mut result = 0usize;
    let answered = unsafe {
        SendMessageTimeoutW(
            search.window,
            WM_NULL,
            0,
            0,
            SMTO_BLOCK | SMTO_ABORTIFHUNG,
            ANSWER_WITHIN.as_millis() as u32,
            &mut result,
        )
    };
    if answered == 0 {
        Answer::Hung
    } else {
        Answer::Answers
    }
}

/// Ask the process whether its window answers.
///
/// Without the platform's window API there is no window to ask, and a heartbeat
/// is the only sign of life there is.
#[cfg(not(target_os = "windows"))]
pub fn ask(_pid: u32) -> Answer {
    Answer::NoWindow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet_for(seconds: u64) -> Option<Duration> {
        Some(Duration::from_secs(seconds))
    }

    #[test]
    fn a_hung_window_that_stopped_beating_is_one_report() {
        let mut episodes = Episodes::new();
        assert_eq!(
            episodes.observe(quiet_for(30), Answer::Hung),
            Episode::Frozen {
                quiet_for: Duration::from_secs(30)
            }
        );
        assert_eq!(
            episodes.observe(quiet_for(31), Answer::Hung),
            Episode::Quiet,
            "a second look at the same freeze writes nothing"
        );
    }

    #[test]
    fn a_window_that_answers_was_only_asleep() {
        let mut episodes = Episodes::new();
        assert_eq!(
            episodes.observe(quiet_for(600), Answer::Answers),
            Episode::Quiet,
            "a machine resumed from sleep is not a frozen program"
        );
    }

    #[test]
    fn beating_again_ends_the_episode() {
        let mut episodes = Episodes::new();
        episodes.observe(quiet_for(30), Answer::Hung);
        assert_eq!(
            episodes.observe(quiet_for(0), Answer::Answers),
            Episode::Recovered
        );
        // The next freeze is a new episode.
        assert!(matches!(
            episodes.observe(quiet_for(40), Answer::Hung),
            Episode::Frozen { .. }
        ));
    }

    #[test]
    fn a_quiet_application_that_never_beat_is_not_frozen() {
        let mut episodes = Episodes::new();
        assert_eq!(episodes.observe(None, Answer::Hung), Episode::Quiet);
    }

    #[test]
    fn a_process_without_a_window_is_not_called_hung() {
        let mut episodes = Episodes::new();
        assert_eq!(episodes.observe(quiet_for(60), Answer::NoWindow), Episode::Quiet);
    }

    #[test]
    fn this_process_has_no_window_and_is_not_hung() {
        let answer = ask(std::process::id());
        assert_ne!(answer, Answer::Hung);
    }
}
