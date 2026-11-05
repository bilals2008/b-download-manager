//! Test-only helpers shared by the media modules.
//!
//! The remux and verification paths are fully native now; there is no external
//! video tool to locate or run. The one thing the tests still share is a lock
//! around the `BDM_FFMPEG` override the older tests used, so a stray variable
//! from one test cannot leak into another.

/// One lock for every test that manipulates a process-wide override. Tests
/// share a process, so an environment variable set by one must not reach
/// another module's test.
#[cfg(test)]
pub fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
