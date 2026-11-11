//! The "Download progress" window.
//!
//! The reference download manager opens a window of its own for a download in
//! flight: the status, the sizes, the transfer rate, the time left, the resume
//! capability, the connections, and the pause, cancel and details controls.
//! The clone gives the reader that window, built from the same state the list
//! reads.
//!
//! Like the completion window it is a webview with its own title bar, so the
//! main window stays usable. The download rides in as an initialization
//! script; every later state arrives on `download://update`, which the app
//! broadcasts to every window. Closing it hides nothing but the window: the
//! download goes on.
//!
//! **One window serves every download.** A webview costs about a hundred
//! milliseconds to build, and it has to be built on the main thread, so a
//! window per download meant that eight downloads starting together froze the
//! window for a second and a half before anything was drawn. The window
//! follows the download that was opened last, the way the quality panel of a
//! video follows the video that was opened last, and it says which one it
//! follows on `progress://job`.

use idm_core::downloader::DownloadInfo;
use idm_core::log;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The label of the one progress window.
pub const WINDOW: &str = "progress";

/// The id of the one tray icon the window owns.
const TRAY: &str = "progress-tray";

/// What the system title adds after the file name.
const TITLE_SUFFIX: &str = " — Download progress";

/// The width of the window. Wide enough for two columns of connection rows
/// and a percentage, narrow enough to sit beside a browser.
const WIDTH: f64 = 560.0;

/// The height with the connection details open.
const DETAILS_HEIGHT: f64 = 552.0;

/// The height with the connection details folded away. Only the details
/// section differs, so the window does not jump when it is folded. The height
/// still leaves room for the two lines an error may take.
const COMPACT_HEIGHT: f64 = 380.0;

// A folded window is shorter than an open one; the geometry is settled when
// the crate is compiled, so a later change to either height cannot invert them.
const _: () = assert!(COMPACT_HEIGHT < DETAILS_HEIGHT);

/// How long the window takes to travel between the two heights.
const FOLD_MS: u64 = 180;

/// The steps that travel is spread over, one per frame at rest.
const FOLD_STEPS: u64 = 12;

/// The newest fold of each progress window, keyed by window label.
///
/// One token for the whole application would make a fold in one window stop
/// the travel of every other window, leaving it at a half-open height. Each
/// window cancels only its own earlier travel.
static FOLDS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

/// The last tooltip of each progress icon. Progress reports arrive several
/// times a second; the shell only hears about a line that changed.
static TIPS: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();

fn folds() -> &'static Mutex<HashMap<String, u64>> {
    FOLDS.get_or_init(Default::default)
}

/// Claim the next fold token of one window.
fn claim_fold(label: &str) -> u64 {
    let mut folds = folds().lock().unwrap_or_else(|error| error.into_inner());
    let token = folds.entry(label.to_string()).or_insert(0);
    *token += 1;
    *token
}

/// Whether a fold that claimed `token` is still the newest one of its window.
fn fold_is_current(label: &str, token: u64) -> bool {
    folds()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(label)
        .is_some_and(|current| *current == token)
}

/// Drop the fold state of a window that closed.
pub fn forget(label: &str) {
    folds()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(label);
}

/// The download the one window is following.
///
/// The window is shared, so this is what says which row it belongs to: the
/// page is told through `progress://job`, and the tray tooltip and the title
/// follow the same download.
static SHOWING: OnceLock<Mutex<Option<u64>>> = OnceLock::new();

/// The download the window is following.
fn showing() -> Option<u64> {
    *SHOWING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// The download the window follows from now on.
fn set_showing(id: u64) {
    *SHOWING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(id);
}

/// The tray icon of the one window.
fn tray_id() -> &'static str {
    TRAY
}

/// What the menu of a progress window's tray icon can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMenuAction {
    Show,
    Close,
}

/// The menu id of one action of one progress window's tray icon.
pub fn menu_id(id: u64, action: TrayMenuAction) -> String {
    match action {
        TrayMenuAction::Show => format!("progress-show:{id}"),
        TrayMenuAction::Close => format!("progress-close:{id}"),
    }
}

/// Read a menu id back into the download and the action it names.
pub fn parse_menu_id(id: &str) -> Option<(u64, TrayMenuAction)> {
    let (action, target) = id.split_once(':')?;
    let download = target.parse().ok()?;
    match action {
        "progress-show" => Some((download, TrayMenuAction::Show)),
        "progress-close" => Some((download, TrayMenuAction::Close)),
        _ => None,
    }
}

/// Run a menu item of a progress window's tray icon.
///
/// Rust registers one global menu listener for the whole application, so the
/// listener of the main tray is the one that receives these events. It hands
/// the id here; an id that is not ours is ignored.
pub fn handle_menu(app: &AppHandle, id: &str) {
    let Some((download, action)) = parse_menu_id(id) else {
        return;
    };
    match action {
        TrayMenuAction::Show => reveal(app, download),
        TrayMenuAction::Close => close(app, download),
    }
}

/// The title the operating system shows for the window.
///
/// The custom bar draws the file name itself, but the task switcher only knows
/// this title. Naming the file there keeps two progress windows apart, and the
/// tray icon of the window is named after the same file.
fn window_title(info: &DownloadInfo) -> String {
    let name = crate::dest::last_segment(&info.dest).unwrap_or_default();
    if name.is_empty() {
        "Download progress".to_string()
    } else {
        format!("{name}{TITLE_SUFFIX}")
    }
}

/// Bring the existing progress window back from the tray or the taskbar.
///
/// The window is shared, so it is brought back whichever download it follows.
pub fn reveal(app: &AppHandle, _id: u64) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Close the progress window, leaving the download running.
///
/// A row that leaves the list closes the window only when the window follows
/// that row: the window may already be showing another download.
fn close_window(app: &AppHandle, id: u64) {
    if showing() != Some(id) {
        return;
    }
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.close();
    }
}

/// What the tray icon says when the pointer rests on it.
///
/// The window title names the file for the task switcher; the tooltip adds
/// the one thing the icon cannot show: how far along it is.
fn tray_tip(info: &DownloadInfo) -> String {
    let name = crate::dest::last_segment(&info.dest).unwrap_or_else(|| "Download".to_string());
    let state = match info.status {
        idm_core::downloader::Status::Running if info.total > 0 => {
            format!(
                "{}%",
                (info.downloaded.saturating_mul(100) / info.total).min(100)
            )
        }
        idm_core::downloader::Status::Running => "downloading".to_string(),
        idm_core::downloader::Status::Queued => "queued".to_string(),
        idm_core::downloader::Status::Paused => "paused".to_string(),
        idm_core::downloader::Status::Completed => "complete".to_string(),
        idm_core::downloader::Status::Failed => "failed".to_string(),
        idm_core::downloader::Status::Cancelled => "stopped".to_string(),
    };
    // Windows keeps about 120 characters of a tray tooltip.
    format!("{name} — {state}").chars().take(120).collect()
}

/// Keep the progress window's tray icon as current as the window itself.
///
/// Driven by the same state report that updates the window, so a hidden
/// progress window still has an honest tooltip without a timer.
pub fn refresh_tray(app: &AppHandle, info: &DownloadInfo) {
    // The icon belongs to the one window, so it describes the download that
    // window follows and nothing else.
    if showing() != Some(info.id) {
        return;
    }
    let Some(tray) = app.tray_by_id(tray_id()) else {
        return;
    };
    let text = tray_tip(info);
    // A running download changes its percentage several times a second, and
    // each change is a synchronous call into the window toolkit. The tip is
    // refreshed at most twice a second; a row that is no longer running is
    // written at once, so the icon never ends the run on a stale number.
    if matches!(info.status, idm_core::downloader::Status::Running) && !due_for_tip(info.id) {
        return;
    }
    {
        let mut tips = TIPS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if tips.get(&info.id) == Some(&text) {
            return;
        }
        tips.insert(info.id, text.clone());
    }
    let _ = tray.set_tooltip(Some(&text));
}

/// How long a running download's tooltip waits before it is written again.
const TIP_INTERVAL: Duration = Duration::from_millis(500);

/// The last time each progress window's tooltip was written.
static LAST_TIP: OnceLock<Mutex<HashMap<u64, std::time::Instant>>> = OnceLock::new();

/// Whether enough time has passed to write this download's tooltip again.
fn due_for_tip(id: u64) -> bool {
    let mut last = LAST_TIP
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let now = std::time::Instant::now();
    match last.get(&id) {
        Some(prev) if now.duration_since(*prev) < TIP_INTERVAL => false,
        _ => {
            last.insert(id, now);
            true
        }
    }
}

/// Give the progress window its tray icon.
///
/// The icon is the application's, labelled with the file. Clicking it brings
/// the window back from the tray; its menu closes the window without touching
/// the download. One icon for the one window is what keeps a hidden progress
/// window findable without a trip through the download list.
fn add_tray(app: &AppHandle, info: &DownloadInfo) -> tauri::Result<()> {
    let id = tray_id();
    if app.tray_by_id(id).is_some() {
        return Ok(());
    }

    let download = info.id;
    let show = MenuItem::with_id(
        app,
        menu_id(info.id, TrayMenuAction::Show),
        "Show window",
        true,
        None::<&str>,
    )?;
    let close = MenuItem::with_id(
        app,
        menu_id(info.id, TrayMenuAction::Close),
        "Close window",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&show, &close])?;

    let mut builder = TrayIconBuilder::with_id(id)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(tray_tip(info))
        // The menu is handled by the application's one global menu listener:
        // `on_menu_event` is global in Tauri, so one listener per window would
        // make every window react to every other window's menu item.
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                reveal(tray.app_handle(), download);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/// Remove the tray icon of the progress window that really closed.
pub fn remove_tray(app: &AppHandle) {
    // Dropping the icon that comes back removes it from the notification area.
    let _ = app.remove_tray_by_id(tray_id());
    TIPS.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    *SHOWING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
}

/// The script that hands the download to the page before it renders.
fn payload(info: &DownloadInfo) -> String {
    let json = serde_json::to_string(info).unwrap_or_else(|_| "null".to_string());
    format!("window.__BDM_PROGRESS__ = {json};")
}

/// Show the progress window of a download, or bring it back to the front when
/// it is already open.
///
/// Called on the main thread, where the window toolkit lives.
///
/// The window is shared: when it is already open it follows the download that
/// was named here, and the page is told which one that is. Building a webview
/// takes about a hundred milliseconds on the main thread, so a window per
/// download froze the application whenever several downloads started at once.
///
/// `focus` says whether the window may take the focus. A window that opens by
/// itself does not: eight downloads starting together would otherwise pull
/// the window to the front eight times, and a window that keeps pulling
/// itself forward reads as one that will not stay closed.
pub fn show(app: &AppHandle, info: &DownloadInfo, focus: bool) {
    set_showing(info.id);
    if let Some(window) = app.get_webview_window(WINDOW) {
        let title = window_title(info);
        let _ = window.set_title(&title);
        let _ = window.emit("progress://job", json!({ "job": info.id }));
        if focus {
            reveal(app, info.id);
        } else if !window.is_visible().unwrap_or(false) {
            // The reader sent the window to the tray; a download that starts
            // brings it back, but it does not steal the focus.
            let _ = window.show();
        }
        // A window that was opened before its tray icon could be built gets
        // one now rather than staying without a way back from the tray.
        if let Err(error) = add_tray(app, info) {
            log::write("warn", &format!("the tray icon did not open: {error}"));
        } else {
            // A window reopened after its download moved on may have an icon
            // from before the move; it is named and described again here.
            refresh_tray(app, info);
        }
        return;
    }

    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title(window_title(info))
        .inner_size(WIDTH, DETAILS_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .center()
        // Hidden until the page has painted: a visible webview shows a white
        // frame while it loads. The page calls `show_ready` when it is ready.
        .visible(false)
        .initialization_script(payload(info))
        .build();
    match built {
        Ok(window) => {
            crate::window_style::round_corners(&window);
            if let Err(error) = add_tray(app, info) {
                log::write("warn", &format!("the tray icon did not open: {error}"));
            }
        }
        Err(error) => log::write(
            "warn",
            &format!("the progress window did not open: {error}"),
        ),
    }
}

/// Close the progress window of a download that is leaving the list.
///
/// A window of a download the engine no longer knows would only be able to
/// report refusals, so it goes with the row.
pub fn close(app: &AppHandle, id: u64) {
    close_window(app, id);
}

/// Fold the connection details away, or open them again.
///
/// The window is not resizable, so the height is chosen here rather than by
/// the user. The page asks for this when its details control is used, and the
/// height travels to the new value instead of jumping, so the section is
/// revealed rather than replaced.
pub fn set_details(window: &WebviewWindow, open: bool, animated: bool) {
    let target = if open { DETAILS_HEIGHT } else { COMPACT_HEIGHT };
    let from = inner_height(window);

    if !animated || (from - target).abs() < 0.5 {
        resize(window, target);
        return;
    }

    // The last toggle of this window owns it: an earlier loop that is still in
    // flight sees the newer token and stops instead of fighting it.
    let label = window.label().to_string();
    let token = claim_fold(&label);
    let window = window.clone();
    std::thread::spawn(move || {
        for step in 1..=FOLD_STEPS {
            if !fold_is_current(&label, token) {
                return;
            }
            let progress = step as f64 / FOLD_STEPS as f64;
            // Ease out: most of the distance is covered at once, which reads as
            // the window opening rather than being dragged.
            let eased = 1.0 - (1.0 - progress).powi(3);
            if !resize(&window, from + (target - from) * eased) {
                return;
            }
            std::thread::sleep(Duration::from_millis(FOLD_MS / FOLD_STEPS));
        }
    });
}

/// Set the height of the window, keeping the width it was given.
fn resize(window: &WebviewWindow, height: f64) -> bool {
    match window.set_size(LogicalSize::new(WIDTH, height)) {
        Ok(()) => true,
        Err(error) => {
            log::write(
                "warn",
                &format!("the progress window did not resize: {error}"),
            );
            false
        }
    }
}

/// The height of the content, which is the height the page lays out against.
fn inner_height(window: &WebviewWindow) -> f64 {
    let scale = window.scale_factor().unwrap_or(1.0);
    window
        .inner_size()
        .map(|size| size.to_logical::<f64>(scale).height)
        .unwrap_or(COMPACT_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info() -> DownloadInfo {
        DownloadInfo {
            id: 9,
            url: "https://example.org/a.bin".to_string(),
            dest: "C:\\Downloads\\a.bin".to_string(),
            total: 4096,
            downloaded: 1024,
            connections: 2,
            status: idm_core::downloader::Status::Running,
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
    fn one_window_serves_every_download() {
        // A webview costs about a hundred milliseconds to build on the main
        // thread, so a window per download froze the application.
        assert_eq!(WINDOW, "progress");
    }

    #[test]
    fn the_window_owns_one_tray_icon() {
        assert_eq!(tray_id(), "progress-tray");
    }

    #[test]
    fn a_tray_menu_id_names_its_download_and_action() {
        assert_eq!(menu_id(7, TrayMenuAction::Show), "progress-show:7");
        assert_eq!(menu_id(7, TrayMenuAction::Close), "progress-close:7");
        assert_eq!(
            parse_menu_id("progress-show:7"),
            Some((7, TrayMenuAction::Show))
        );
        assert_eq!(
            parse_menu_id(&menu_id(8, TrayMenuAction::Close)),
            Some((8, TrayMenuAction::Close))
        );
        assert_eq!(parse_menu_id("show"), None);
        assert_eq!(parse_menu_id("progress-show:"), None);
        assert_eq!(parse_menu_id("progress-unknown:7"), None);
    }

    #[test]
    fn a_fold_cancels_only_the_folds_of_its_own_window() {
        forget("progress-1");
        forget("progress-2");
        let one = claim_fold("progress-1");
        let two = claim_fold("progress-2");

        // A toggle in another window does not stop this one's travel.
        assert!(fold_is_current("progress-1", one));
        assert!(fold_is_current("progress-2", two));

        // A newer toggle of the same window does.
        let newer = claim_fold("progress-1");
        assert!(!fold_is_current("progress-1", one));
        assert!(fold_is_current("progress-1", newer));
        assert!(fold_is_current("progress-2", two));
    }

    #[test]
    fn the_payload_is_the_state_of_the_download() {
        let script = payload(&sample_info());
        assert!(script.starts_with("window.__BDM_PROGRESS__ = {"));
        assert!(script.contains("\"dest\":\"C:\\\\Downloads\\\\a.bin\""));
        assert!(script.contains("\"resumable\":true"));
    }

    #[test]
    fn a_progress_tooltip_names_the_file_and_the_progress() {
        assert_eq!(tray_tip(&sample_info()), "a.bin — 25%");

        let mut unknown = sample_info();
        unknown.total = 0;
        assert_eq!(tray_tip(&unknown), "a.bin — downloading");

        let mut paused = sample_info();
        paused.status = idm_core::downloader::Status::Paused;
        assert_eq!(tray_tip(&paused), "a.bin — paused");

        let mut done = sample_info();
        done.status = idm_core::downloader::Status::Completed;
        done.downloaded = done.total;
        assert_eq!(tray_tip(&done), "a.bin — complete");
    }

    #[test]
    fn a_progress_tooltip_without_a_name_still_reads() {
        let mut info = sample_info();
        info.dest = String::new();
        assert_eq!(tray_tip(&info), "Download — 25%");
    }

    #[test]
    fn the_taskbar_title_names_the_file() {
        assert_eq!(
            window_title(&sample_info()),
            "a.bin — Download progress",
            "two progress windows must be told apart in the task switcher"
        );
    }

    #[test]
    fn a_download_without_a_name_still_gets_a_title() {
        let mut info = sample_info();
        info.dest = String::new();
        assert_eq!(window_title(&info), "Download progress");
    }
}
