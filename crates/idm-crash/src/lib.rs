//! The crash monitor and the freeze detector.
//!
//! A second process waits beside the application and writes what happened
//! when it dies. The application never writes its own dump.

/// What the monitor found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The application ended on its own.
    Exited,
    /// The application stopped answering.
    Frozen,
}

/// Whether a process is still there.
pub fn is_running(pid: u32) -> bool {
    !matches!(process_state(pid), None)
}

/// The name of the state of a process, when the system can tell.
pub fn process_state(pid: u32) -> Option<String> {
    let _ = pid;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_that_is_not_there_is_not_running() {
        assert!(!is_running(0));
    }
}
