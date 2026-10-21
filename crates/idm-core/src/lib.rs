//! The download engine.
//!
//! The engine knows nothing about windows: the application hands it a
//! reporter and it reports the state of the downloads back. Everything here
//! can be run without a display, which is what the tests and the headless
//! client do.

pub mod disk;
pub mod downloader;
pub mod ftp;
pub mod http;
pub mod limits;
pub mod log;
pub mod segmenter;
pub mod session;
pub mod transfer;
pub mod verify;
