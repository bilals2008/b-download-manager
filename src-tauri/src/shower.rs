//! Build windows one at a time, with a beat between them.
//!
//! A webview is built on the main thread, the one the window toolkit runs on.
//! Several windows built in the same moment — a batch of downloads starting or
//! finishing together — hold that thread for their combined build time, and
//! the whole application stops answering. The shower hands each build its own
//! moment and waits for the next, so the main thread stays responsive between
//! windows.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// How long the shower waits between two window builds.
const INTERVAL: Duration = Duration::from_millis(220);

/// The moment the last window build started.
static LAST: OnceLock<Mutex<Instant>> = OnceLock::new();

/// Run `build` on the main thread, after any earlier build has had its moment.
pub fn build(app: &AppHandle, build: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let wait = {
                let mut last = LAST
                    .get_or_init(|| Mutex::new(Instant::now() - INTERVAL))
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let since = last.elapsed();
                if since >= INTERVAL {
                    *last = Instant::now();
                    None
                } else {
                    Some(INTERVAL - since)
                }
            };
            match wait {
                None => break,
                Some(delay) => tokio::time::sleep(delay).await,
            }
        }
        let value = app.clone();
        let _ = app.run_on_main_thread(move || build(&value));
    });
}
