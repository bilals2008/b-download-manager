//! The commands the windows call.
//!
//! Each file is one family: the downloads themselves, the queue, the saved
//! preferences, and the system services a window asks for. [`super::run`]
//! registers them; nothing here builds a window of its own beyond asking the
//! window modules to.

pub(crate) mod downloads;
pub(crate) mod preferences;
pub(crate) mod queue;
pub(crate) mod system;
