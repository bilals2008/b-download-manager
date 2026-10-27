//! The application's own tray icon.
//!
//! The icon is the application when no window is in sight: it brings the main
//! window back, holds or lets go of the downloads, and is the one deliberate
//! way out. It is also the only place the application says, while nobody is
//! looking, what the downloads are doing; the tooltip follows the same
//! `download://update` reports the lists use, so it needs no timer of its own.
//!
//! Each progress window owns a tray icon of its own (`progress.rs`). This one
//! belongs to the application and outlives every window.

use std::sync::{Mutex, OnceLock};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use idm_core::downloader::{DownloadInfo, Status};
use idm_core::log;

use crate::actions;
use crate::state::AppState;

/// The id the rest of the application uses to find this icon.
pub const TRAY_ID: &str = "main-tray";

/// The name the tooltip starts with, matching the window title.
const NAME: &str = "B Download Manager";

/// Windows keeps roughly 120 characters of a tray tooltip. The text is cut
/// before it is handed over, so a long file name cannot push the numbers out.
const TIP_LIMIT: usize = 120;

/// What the downloads are doing, counted once for the tooltip and the close
/// question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Activity {
    pub running: usize,
    pub queued: usize,
    pub paused: usize,
    /// The bytes and sizes of the running downloads only.
    pub downloaded: u64,
    pub total: u64,
    /// The first running file, which is the one a single-download tooltip
    /// names.
    pub first_name: Option<String>,
    /// Set when a running download has no known size, so a percentage would
    /// be a guess.
    pub unknown_size: bool,
}

impl Activity {
    fn percent(&self) -> Option<u64> {
        if self.running == 0 || self.unknown_size || self.total == 0 {
            return None;
        }
        Some((self.downloaded.saturating_mul(100) / self.total).min(100))
    }
}

/// Count the downloads as the tray should describe them.
pub fn activity(rows: impl IntoIterator<Item = DownloadInfo>) -> Activity {
    let mut out = Activity::default();
    for info in rows {
        match info.status {
            Status::Running => {
                out.running += 1;
                out.downloaded = out.downloaded.saturating_add(info.downloaded);
                if info.total == 0 {
                    out.unknown_size = true;
                } else {
                    out.total = out.total.saturating_add(info.total);
                }
                if out.first_name.is_none() {
                    out.first_name = crate::dest::last_segment(&info.dest);
                }
            }
            Status::Queued => out.queued += 1,
            Status::Paused => out.paused += 1,
            _ => {}
        }
    }
    out
}

/// The line the tray tooltip shows.
pub fn tooltip_text(activity: &Activity) -> String {
    let text = if activity.running == 1 && activity.queued == 0 {
        let name = activity.first_name.as_deref().unwrap_or("Download");
        let state = match activity.percent() {
            Some(percent) => format!("{percent}%"),
            None => "downloading".to_string(),
        };
        let mut text = format!("{name} — {state}");
        if activity.paused > 0 {
            text.push_str(&format!(" · {} paused", activity.paused));
        }
        if activity.queued > 0 {
            text.push_str(&format!(" · {} queued", activity.queued));
        }
        text
    } else if activity.running > 0 {
        let mut text = format!("{NAME} — {} downloading", activity.running);
        if let Some(percent) = activity.percent() {
            text.push_str(&format!(" · {percent}%"));
        }
        if activity.paused > 0 {
            text.push_str(&format!(" · {} paused", activity.paused));
        }
        if activity.queued > 0 {
            text.push_str(&format!(" · {} queued", activity.queued));
        }
        text
    } else if activity.paused > 0 && activity.queued > 0 {
        format!(
            "{NAME} — {} paused · {} queued",
            activity.paused, activity.queued
        )
    } else if activity.paused > 0 {
        format!(
            "{NAME} — {} paused",
            count(activity.paused, "download", "downloads")
        )
    } else if activity.queued > 0 {
        format!(
            "{NAME} — {} queued",
            count(activity.queued, "download", "downloads")
        )
    } else {
        format!("{NAME} — no active downloads")
    };

    text.chars().take(TIP_LIMIT).collect()
}

fn count(number: usize, one: &str, many: &str) -> String {
    format!("{number} {}", if number == 1 { one } else { many })
}

/// The last line written to the icon, so an unchanged tooltip is not sent to
/// the shell again on every progress report.
static LAST_TIP: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn remember(text: &str) -> bool {
    let mut last = LAST_TIP
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if last.as_deref() == Some(text) {
        return false;
    }
    *last = Some(text.to_string());
    true
}

/// Build the application tray and its menu.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show B Download Manager", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause-all", "Pause all downloads", true, None::<&str>)?;
    let resume = MenuItem::with_id(
        app,
        "resume-all",
        "Resume paused downloads",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &pause, &resume, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(format!("{NAME} — no active downloads"));

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "pause-all" => pause_all(app),
            "resume-all" => resume_all(app),
            "quit" => crate::lifecycle::shutdown(app),
            // The menu of a progress window's own tray icon. Tauri's menu
            // listener is global, so this is the one place those items are
            // routed from; `handle_menu` ignores an id it does not own.
            id => crate::progress::handle_menu(app, id),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Bring the window back from the tray.
///
/// A window hidden while minimized is still minimized underneath, so it is
/// restored before it is shown; otherwise the tray would appear to do nothing
/// but put a taskbar button back.
pub fn show_window(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        None => log::write(
            "warn",
            "the main window is gone; the tray has nothing to show",
        ),
    }
}

/// Update the tooltip from the current downloads.
///
/// Called whenever a download reports progress and after every action that
/// changes a row's state. It is not a timer: with nothing happening, nothing
/// is written.
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let rows = state
        .list()
        .iter()
        .filter(|download| {
            matches!(
                download.status(),
                Status::Running | Status::Queued | Status::Paused
            )
        })
        .map(|download| download.info())
        .collect::<Vec<_>>();
    let text = tooltip_text(&activity(rows));
    if !remember(&text) {
        return;
    }
    match app.tray_by_id(TRAY_ID) {
        Some(tray) => {
            if let Err(error) = tray.set_tooltip(Some(&text)) {
                log::write("warn", &format!("the tray tooltip did not change: {error}"));
            }
        }
        None => log::write("warn", "the tray icon is not there to be described"),
    }
}

/// Remove the application icon. Used once, on the way out.
pub fn remove(app: &AppHandle) {
    let _ = app.remove_tray_by_id(TRAY_ID);
}

/// Hold every running or waiting download. Paused rows are left alone.
fn pause_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut held = 0usize;
    for download in state.list() {
        if matches!(download.status(), Status::Running | Status::Queued) {
            match actions::pause(app, &state, download.id) {
                Ok(()) => held += 1,
                Err(error) => log::write(
                    "warn",
                    &format!("the tray could not pause download {}: {error}", download.id),
                ),
            }
        }
    }
    log::write("info", &format!("the tray held back {held} download(s)"));
    refresh(app);
}

/// Let paused and waiting downloads run. Finished rows stay finished.
fn resume_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut started = 0usize;
    for download in state.list() {
        if matches!(download.status(), Status::Paused | Status::Queued) {
            match actions::resume(app, &state, download.id) {
                Ok(()) => started += 1,
                Err(error) => log::write(
                    "warn",
                    &format!(
                        "the tray could not resume download {}: {error}",
                        download.id
                    ),
                ),
            }
        }
    }
    log::write(
        "info",
        &format!("the tray started {started} paused download(s)"),
    );
    refresh(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, status: Status, downloaded: u64, total: u64) -> DownloadInfo {
        DownloadInfo {
            id: 1,
            url: format!("https://example.org/{name}"),
            dest: format!("C:\\Downloads\\{name}"),
            total,
            downloaded,
            connections: 2,
            status,
            error: None,
            referer: None,
            segments: vec![],
            resumable: Some(true),
            speed_limit: 0,
            category: None,
            description: None,
        }
    }

    #[test]
    fn an_idle_tray_says_so() {
        assert_eq!(
            tooltip_text(&Activity::default()),
            "B Download Manager — no active downloads"
        );
    }

    #[test]
    fn one_download_is_named_with_its_progress() {
        let activity = activity([info("a.bin", Status::Running, 500, 1000)]);
        assert_eq!(tooltip_text(&activity), "a.bin — 50%");
    }

    #[test]
    fn an_unknown_size_is_not_turned_into_a_percentage() {
        let activity = activity([info("a.bin", Status::Running, 500, 0)]);
        assert_eq!(tooltip_text(&activity), "a.bin — downloading");
    }

    #[test]
    fn several_downloads_are_counted_with_the_whole_percentage() {
        let activity = activity([
            info("a.bin", Status::Running, 250, 1000),
            info("b.bin", Status::Running, 250, 1000),
            info("c.bin", Status::Paused, 0, 1000),
            info("d.bin", Status::Queued, 0, 0),
        ]);
        assert_eq!(activity.running, 2);
        assert_eq!(activity.paused, 1);
        assert_eq!(activity.queued, 1);
        assert_eq!(activity.running + activity.paused + activity.queued, 4);
        assert_eq!(
            tooltip_text(&activity),
            "B Download Manager — 2 downloading · 25% · 1 paused · 1 queued"
        );
    }

    #[test]
    fn one_unknown_size_keeps_the_whole_percentage_out() {
        let activity = activity([
            info("a.bin", Status::Running, 250, 1000),
            info("b.bin", Status::Running, 250, 0),
        ]);
        assert_eq!(
            tooltip_text(&activity),
            "B Download Manager — 2 downloading"
        );
    }

    #[test]
    fn a_paused_only_tray_does_not_claim_to_be_downloading() {
        let activity = activity([info("a.bin", Status::Paused, 0, 1000)]);
        assert_eq!(
            tooltip_text(&activity),
            "B Download Manager — 1 download paused"
        );
    }

    #[test]
    fn a_tip_that_would_be_too_long_is_cut() {
        let long = "x".repeat(300);
        let activity = activity([info(&long, Status::Running, 1, 2)]);
        let tip = tooltip_text(&activity);
        assert_eq!(tip.chars().count(), TIP_LIMIT);
    }

    #[test]
    fn finished_rows_are_not_active() {
        let activity = activity([
            info("a.bin", Status::Completed, 1000, 1000),
            info("b.bin", Status::Cancelled, 0, 1000),
            info("c.bin", Status::Failed, 0, 1000),
        ]);
        assert_eq!(activity.running + activity.paused + activity.queued, 0);
        assert_eq!(
            tooltip_text(&activity),
            "B Download Manager — no active downloads"
        );
    }
}
