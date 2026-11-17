//! The system services a window asks for: folders, paths, the shell.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::conflict;
use crate::lifecycle;
use crate::state::AppState;

/// Whether a folder can hold a download, and how much room is left.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FolderStatus {
    /// The folder that was checked. Empty input means the system Downloads
    /// folder, and this is the folder it resolved to.
    path: String,
    exists: bool,
    writable: bool,
    free_bytes: Option<u64>,
    /// One sentence for the window when the folder cannot be used.
    error: Option<String>,
}

#[tauri::command]
pub(crate) fn folder_status(app: AppHandle, folder: String) -> FolderStatus {
    let trimmed = folder.trim();
    let path = if trimmed.is_empty() {
        app.path()
            .download_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(trimmed)
    };
    let shown = path.to_string_lossy().to_string();

    if !path.exists() {
        return FolderStatus {
            path: shown,
            exists: false,
            writable: false,
            free_bytes: None,
            error: Some(
                "This folder is not there yet. It is created when the first download starts."
                    .to_string(),
            ),
        };
    }
    if !path.is_dir() {
        return FolderStatus {
            path: shown,
            exists: true,
            writable: false,
            free_bytes: None,
            error: Some("This path is a file, not a folder.".to_string()),
        };
    }

    let writable = can_write(&path);
    FolderStatus {
        free_bytes: free_space(&path),
        error: if writable {
            None
        } else {
            Some("This folder cannot be written to. Choose another one.".to_string())
        },
        path: shown,
        exists: true,
        writable,
    }
}

/// The log used to sit in the data folder next to the index. One rename
/// carries the history over to the log folder the operating system assigns,
/// and never deletes anything: if the new log already exists, the old file is
/// left where it is.
pub(crate) fn carry_log_over_from_data_dir(data: &std::path::Path, logs: &std::path::Path) {
    if data == logs {
        return;
    }
    let old = data.join("idm-clone.log");
    if !old.is_file() {
        return;
    }
    let landed = logs.join("idm-clone.log");
    if landed.exists() {
        return;
    }
    let _ = std::fs::rename(&old, &landed);
}

/// Where the log and the saved download index live.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppPaths {
    data_dir: String,
    log_path: String,
    /// The application's own storage for the bytes of unfinished downloads.
    staging_dir: String,
}

#[tauri::command]
pub(crate) fn app_paths(app: AppHandle, state: State<'_, AppState>) -> AppPaths {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let log_dir = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| dir.join("logs"));
    AppPaths {
        log_path: log_dir.join("idm-clone.log").to_string_lossy().to_string(),
        staging_dir: state
            .staging()
            .map(|staging| staging.root().to_string_lossy().to_string())
            .unwrap_or_default(),
        data_dir: dir.to_string_lossy().to_string(),
    }
}

/// Whether a file can be created in the folder. The test file is removed
/// again; a folder that refuses it cannot hold a download either.
pub(crate) fn can_write(folder: &std::path::Path) -> bool {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let probe = folder.join(format!(".bdm-write-{}-{stamp}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// The room left on the volume of a folder, when the system can tell.
#[cfg(target_os = "windows")]
pub(crate) fn free_space(folder: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = folder
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
    (ok != 0).then_some(free)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn free_space(_folder: &std::path::Path) -> Option<u64> {
    None
}

/// Show the window this page is in, once it has painted.
///
/// The two small windows are built hidden: a webview that is visible while it
/// loads shows an empty white frame first, which reads as a broken window. The
/// page calls this after its first frames, so the window appears with its
/// content already drawn.
#[tauri::command]
pub(crate) fn show_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

/// Paint the window itself in the theme's background.
///
/// What the webview has not drawn yet shows the window's own fill. The webview
/// default is white, so every load or repaint of a hidden window could flash
/// white before the page arrived. The page sends the colour of its
/// `--background` token, and a theme change sends the next one.
#[tauri::command]
pub(crate) fn set_window_background(
    window: tauri::WebviewWindow,
    color: String,
) -> Result<(), String> {
    let (red, green, blue) = parse_hex_color(&color).ok_or("expected a #rgb or #rrggbb colour")?;
    window
        .set_background_color(Some(tauri::window::Color(red, green, blue, 255)))
        .map_err(|error| error.to_string())
}

/// Read the tokens' hex form (`#0b1416`, `#fff`) into the channels Tauri wants.
pub(crate) fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().strip_prefix('#')?;
    let channel = |digits: &str| u8::from_str_radix(digits, 16).ok();
    match hex.len() {
        3 => {
            let cell = |index: usize| channel(&hex[index..index + 1]).map(|value| value * 17);
            Some((cell(0)?, cell(1)?, cell(2)?))
        }
        6 => Some((
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        )),
        _ => None,
    }
}

/// Whether a destination can hold the file, and whether a file is already
/// there. The add dialog asks before it starts, so a bad name is a line in
/// the dialog instead of a failed download.
#[tauri::command]
pub(crate) fn inspect_destination(
    state: State<'_, AppState>,
    dest: String,
) -> Result<conflict::DestinationStatus, String> {
    conflict::inspect(&state, &PathBuf::from(dest.trim()))
}

/// Open a file or a folder with the program the system uses for it.
///
/// The path comes from the download list, so it is a place the user picked.
#[tauri::command]
pub(crate) fn open_path(path: String) -> Result<(), String> {
    let target = PathBuf::from(path.trim());
    if target.as_os_str().is_empty() {
        return Err("a path is required".to_string());
    }
    if !target.exists() {
        return Err(format!("{} is not there yet", target.display()));
    }
    open_in_os(&target)
}

/// Whether an address is a web page rather than something else on the machine.
///
/// The only links this window offers go to a website; a `file:` or a `javascript:`
/// address would be handed to whatever program owns it.
pub(crate) fn is_web_address(url: &str) -> bool {
    !url.chars().any(|c| c.is_control())
        && (url.starts_with("http://") || url.starts_with("https://"))
}

/// Open a web address in the browser the system uses.
///
/// The window is a webview, not a tab: an anchor here would replace the page
/// it is on, so the address goes to the shell instead.
#[tauri::command]
pub(crate) fn open_url(url: String) -> Result<(), String> {
    let address = url.trim();
    if !is_web_address(address) {
        return Err("only a web address can be opened here".to_string());
    }
    open_in_browser(address)
}

/// Hand the path to the shell of the system.
#[cfg(target_os = "windows")]
pub(crate) fn open_in_os(target: &std::path::Path) -> Result<(), String> {
    let mut command = if target.is_dir() {
        let mut command = std::process::Command::new("explorer");
        command.arg(target);
        command
    } else {
        // `explorer` would open the folder of a file, so a file goes through
        // the shell handler that opens it with its own program.
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(target);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the system could not open it: {error}"))
}

#[cfg(target_os = "macos")]
pub(crate) fn open_in_os(target: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the system could not open it: {error}"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn open_in_os(target: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the system could not open it: {error}"))
}

/// Hand a web address to the browser the system uses.
///
/// The three forms differ only in the program and, on Windows, in the handler
/// `rundll32` is asked to load. The address arrives as one argument of one
/// process, never through a shell, so it cannot carry a second command.
pub(crate) fn open_in_browser(address: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");

    command
        .arg(address)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the browser could not be opened: {error}"))
}

/// Leave the application for good, the way the Exit item of the menu does.
///
/// Closing the window only hides it into the tray, so the menu needs a way
/// out. The one shutdown path pauses the running transfers and writes their
/// resumes before the process ends.
#[tauri::command]
pub(crate) fn quit_app(app: AppHandle) {
    lifecycle::shutdown(&app);
}
