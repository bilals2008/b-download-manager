mod actions;
mod bridge;
mod checksum;
mod clipboard;
mod commands;
mod complete;
mod conflict;
pub mod crash;
mod debrid;
mod dest;
mod dns;
mod drive;
mod extract;
mod gallery;
mod grabber;
mod lifecycle;
mod mcp;
mod migrate;
mod network;
mod page_media;
mod prefs;
mod progress;
mod proxy;
mod queue;
mod remote;
mod schedule;
mod settings_store;
mod share;
mod shower;
mod staging;
mod state;
mod toolbars;
mod torrent;
mod tray;
mod updater;
mod video;
mod window_style;

use tauri::{Emitter, Manager, WindowEvent};

use idm_core::session::Index;

use commands::downloads::show_progress_if_requested;
use commands::system::{carry_log_over_from_data_dir, open_in_os};
use state::AppState;

pub(crate) use commands::downloads::*;
pub(crate) use commands::preferences::*;
pub(crate) use commands::queue::*;
pub(crate) use commands::system::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .manage(grabber::Grabber::default())
        .setup(|app| {
            let state = app.state::<AppState>();
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            // The diagnostics log lives in the log folder the operating
            // system assigns to the application, with the data folder as the
            // fallback. Opened first so a start that goes wrong anywhere
            // below has somewhere to be written down.
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| dir.join("logs"));
            let _ = std::fs::create_dir_all(&log_dir);
            idm_core::log::open(log_dir.join("idm-clone.log"));
            carry_log_over_from_data_dir(&dir, &log_dir);
            // What each host allowed the last time it was asked lives beside
            // the settings, so a server that refused ranges is not asked for
            // them again after a restart.
            state.limits().learned.open(&dir.join("host-caps.json"));
            // A second copy of the application watches this one from the
            // outside and writes a report when it ends badly. It is the
            // monitor, not this copy, that writes the report: a program that
            // is already falling over cannot be trusted to write one.
            crash::start(log_dir.clone());
            // The staging tree is the only place an unfinished download may
            // write. Without it a start refuses rather than letting its bytes
            // land beside the finished file.
            let staging = match staging::Staging::resolve(&dir) {
                Ok(tree) => Some(tree),
                Err(error) => {
                    idm_core::log::write(
                        "error",
                        &format!("the staging folder could not be made: {error}"),
                    );
                    None
                }
            };
            // The downloads of an earlier run are brought back, so the list
            // is still there after a restart. The migration runs first: a
            // restored row then points at the staging tree instead of the
            // folder an older release left its parts in.
            let index = Index::new(dir.join("downloads.json"));
            if let Some(tree) = staging.as_ref() {
                let report = migrate::run(&index, tree);
                if report.moved > 0 || report.failed > 0 {
                    idm_core::log::write(
                        "info",
                        &format!(
                            "the storage migration moved {} download(s), left {} where they are",
                            report.moved, report.failed
                        ),
                    );
                }
            }
            state.set_index(index);
            // The engine's settings come back the same way, and are applied
            // before the bridge binds: a link caught the moment the
            // application opens must land in the folder the reader chose,
            // through the proxy the reader set, at the connection counts the
            // reader chose. The webview pushes its own copy over these as it
            // still does; that push is what repairs the file after an
            // upgrade, and the file wins after that. This runs before the
            // downloads of an earlier run are brought back, so a resumed
            // download looks names up through the resolver the reader chose,
            // like a new one.
            state.set_settings_path(dir.join(settings_store::FILE_NAME), dir.clone());
            state.restore_downloads();
            state.restore_queue();
            // The video downloads of an earlier run come back the same way:
            // their history is a file of its own, read before the bridge
            // binds so the list is whole when the window opens.
            state.set_media_index(video::persist::MediaIndex::new(dir.join("media.json")));
            state.restore_media_jobs();
            // The sweep of abandoned staging folders waits until the restored
            // rows exist: a resumable download claims its own folder.
            if let Some(tree) = staging {
                state.set_staging(tree);
                state.sweep_media_folders();
            }
            // The tray and the close button follow a file of their own: they
            // are consulted before a page has loaded, and a webview that has
            // not painted yet must not decide whether closing exits.
            state.set_preferences_path(dir.join(prefs::FILE_NAME));
            // The icon goes up unless the reader took it away in an earlier
            // run. Without it the close and minimize choices fall back to
            // closing and minimizing, which is what `tray_available` says.
            // Putting it up also writes the tooltip of what is already running.
            tray::set_visible(app.handle(), !state.preferences().tray_hidden);
            if let Some(window) = app.get_webview_window("main") {
                window_style::round_corners(&window);
            }
            // The browser extension talks to the application through this.
            bridge::start(app.handle().clone());
            // The adapters are watched, so a chosen adapter that goes away
            // stops the downloads instead of letting them take the normal
            // route.
            network::watch(app.handle().clone());
            // The proxy script is fetched with the same client policy as
            // everything else, and fetched again now and then.
            proxy::start(app.handle().clone());
            // The phone remote is off until the reader turns it on; when it
            // is on, it starts with the application so a paired phone finds
            // it on the same address.
            if state.engine_settings().remote_enabled {
                if let Err(error) = remote::start(app.handle(), &state) {
                    idm_core::log::write(
                        "warn",
                        &format!("the phone remote is on but could not start: {error}"),
                    );
                }
            }
            // A copied link is offered while the application is in the tray.
            clipboard::start(app.handle().clone());
            // The queue's schedule is read on a clock, so a window that opens
            // while nobody is looking still starts the queue.
            schedule::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // A close is never the end of the main webview: it either hides,
            // asks, or takes the one shutdown path. The smaller windows of
            // the application really close.
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() != "main" {
                    return;
                }
                api.prevent_close();
                lifecycle::close_requested(window);
            }
            // The taskbar and an OS shortcut can minimize a window without
            // the title-bar button; hide it here too when that is configured.
            WindowEvent::Resized(_) => {
                if window.label() == "main"
                    && window.is_minimized().unwrap_or(false)
                    && window
                        .app_handle()
                        .state::<AppState>()
                        .preferences()
                        .minimize_to_tray
                {
                    lifecycle::minimize_to_tray(window);
                }
            }
            // The progress window that really closed takes its tray icon with
            // it; one left behind would open nothing.
            WindowEvent::Destroyed => {
                if window.label() == progress::WINDOW {
                    progress::remove_tray(window.app_handle());
                    progress::forget(window.label());
                }
                // The torrent window really closes, and a closed window leaves
                // nothing behind, so the main window says where it went.
                if window.label() == torrent::WINDOW {
                    if let Some(main) = window.app_handle().get_webview_window("main") {
                        let _ = main.emit("torrent://closed", ());
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            clipboard::clipboard_text,
            list_downloads,
            pause_download,
            resume_download,
            cancel_download,
            retry_download,
            redownload_download,
            relocate_download,
            set_download_meta,
            set_download_destination,
            queue_download,
            dequeue_download,
            start_queue,
            stop_queue,
            set_queue_position,
            set_max_simultaneous,
            get_queue,
            schedule::queue_schedule,
            schedule::set_queue_schedule,
            remote::remote_set_enabled,
            remote::remote_status,
            remote::remote_pairing_code,
            remote::remote_revoke,
            torrent::torrent_start,
            torrent::torrent_lookup,
            torrent::torrent_jobs,
            torrent::torrent_job,
            torrent::torrent_pause,
            torrent::torrent_resume,
            torrent::torrent_set_seeding,
            torrent::torrent_seeding,
            torrent::set_torrent_seeding,
            torrent::torrent_remove,
            torrent::torrent_recheck,
            torrent::torrent_set_files,
            torrent::torrent_add_trackers,
            torrent::torrent_remove_trackers,
            torrent::torrent_announce_now,
            torrent::torrent_check_trackers,
            torrent::torrent_pause_all,
            torrent::torrent_resume_all,
            torrent::torrent_settings,
            torrent::set_torrent_settings,
            torrent::torrent_port_mapping,
            torrent::show_torrent_window,
            delete_download_file,
            remove_download,
            set_speed_limit,
            set_proxy,
            set_download_speed_limit,
            set_download_category,
            open_progress_window,
            set_progress_details,
            set_site_connections,
            reset_learned_host,
            site_connections,
            set_max_connections,
            desktop_preferences,
            set_minimize_to_tray,
            set_close_to_tray,
            set_tray_hidden,
            set_tray_dark_mode,
            set_tray_appearance,
            set_url_protocols,
            lifecycle::minimize_window,
            lifecycle::hide_main_window,
            set_show_complete_dialog,
            set_show_progress_on_start,
            set_show_video_window_on_start,
            set_browser_connections,
            set_clipboard_capture,
            set_extract_after_done,
            set_delete_archive_after_extract,
            set_launch_on_startup,
            set_browser_policy,
            set_video_button_settings,
            video_button_settings,
            set_browser_theme,
            set_category_rules,
            saved_settings,
            browser_status,
            folder_status,
            app_paths,
            validate_proxy,
            dns::set_dns,
            dns::dns_settings,
            dns::test_dns,
            proxy::set_proxy_script,
            proxy::reload_proxy_script,
            proxy::proxy_script_state,
            network::network_adapters,
            network::network_state,
            network::set_network_adapter,
            set_browser_folder,
            quit_app,
            default_destination,
            probe_link,
            inspect_destination,
            gallery::gallery_inspect,
            gallery::gallery_download,
            grabber::open_grabber_window,
            grabber::grabber_start,
            grabber::grabber_cancel,
            grabber::grabber_state,
            grabber::grabber_download,
            show_ready,
            set_window_background,
            open_path,
            checksum::verify_download_checksum,
            crash::last_freeze_report,
            debrid::debrid_services,
            debrid::debrid_sign_in,
            debrid::debrid_sign_out,
            debrid::debrid_account,
            debrid::debrid_test,
            debrid::debrid_unrestrict,
            debrid::debrid_set_auto,
            debrid::debrid_auto,
            debrid::debrid_ask_unlock,
            mcp::mcp_enabled,
            mcp::set_mcp_enabled,
            extract::extract_download,
            extract::extract_download_with_password,
            extract::extract_pending,
            open_url,
            share::share_app,
            updater::check_update,
            updater::install_update,
            updater::restart_app,
            toolbars::list_toolbar_packs,
            toolbars::install_toolbar_pack,
            toolbars::remove_toolbar_pack,
            toolbars::reveal_toolbar_packs,
            video::capture::media_job,
            video::capture::media_jobs,
            video::capture::media_forget,
            video::capture::media_download,
            video::capture::media_allow_cookies,
            video::capture::media_cancel,
            video::capture::media_resume,
            video::capture::media_redownload,
            video::capture::media_reparse,
            video::capture::open_video_window,
            video::capture::page::page_media_open,
            video::capture::open_media_progress,
            video::capture::video_helper_status,
            video::capture::video_helper_install,
            video::capture::video_helper_import,
            video::capture::video_helper_forget,
        ])
        .build(tauri::generate_context!())
        .expect("error while building B Download Manager")
        .run(|app, event| {
            // The operating system can end the process without a close
            // request (logoff, shutdown). The deliberate paths use
            // `lifecycle::shutdown`; this is the last chance for the rest.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                torrent::shutdown();
                lifecycle::save_on_exit(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_path_is_refused() {
        assert!(open_path("   ".to_string()).is_err());
    }

    #[test]
    fn only_a_web_address_is_opened_in_the_browser() {
        // Each of these is refused before anything is spawned, so the test
        // never opens a window of its own.
        assert!(!is_web_address(""));
        assert!(!is_web_address("ftp://example.org/file"));
        assert!(!is_web_address("file:///C:/Windows/system.ini"));
        assert!(!is_web_address("javascript:alert(1)"));
        assert!(!is_web_address("https://example.org\0"));
        assert!(is_web_address("https://example.org/"));
        assert!(is_web_address("http://localhost:1420"));
        assert!(open_url("mailto:someone@example.org".to_string()).is_err());
    }

    #[test]
    fn a_start_request_reads_the_names_the_window_sends() {
        let request: StartRequest = serde_json::from_value(serde_json::json!({
            "url": "https://example.org/film.mp4",
            "dest": "D:\\Downloads\\film.mp4",
            "connections": 8,
            "referer": null,
            "queue": false,
            "category": "Video",
            "description": null,
            "conflict": "rename",
        }))
        .expect("a start request");

        assert_eq!(request.url, "https://example.org/film.mp4");
        assert_eq!(request.connections, Some(8));
        assert_eq!(request.queue, Some(false));
        assert_eq!(request.category.as_deref(), Some("Video"));
        assert_eq!(request.conflict, Some(conflict::Choice::Rename));
    }

    #[test]
    fn a_start_request_from_an_older_window_asks_the_destination_question() {
        let request: StartRequest = serde_json::from_value(serde_json::json!({
            "url": "https://example.org/film.mp4",
            "dest": "D:\\Downloads\\film.mp4",
        }))
        .expect("a start request");

        assert_eq!(request.conflict, None, "an older window has no answer yet");
        assert_eq!(request.conflict.unwrap_or_default(), conflict::Choice::Ask);
    }

    #[test]
    fn a_theme_colour_is_read_for_the_window() {
        assert_eq!(parse_hex_color("#0b1416"), Some((11, 20, 22)));
        assert_eq!(parse_hex_color(" #fff "), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("0b1416"), None);
        assert_eq!(parse_hex_color("#12345"), None);
    }

    #[test]
    fn a_path_that_is_not_there_is_refused() {
        let missing = std::env::temp_dir().join("idm-clone-missing-8c41.bin");
        let error = open_path(missing.to_string_lossy().to_string()).unwrap_err();
        assert!(error.contains("not there yet"), "the error was: {error}");
    }

    #[test]
    fn a_capture_policy_is_cleaned_at_the_boundary() {
        let policy = normalize_policy(state::BrowserPolicy {
            capture: true,
            ask: true,
            types: vec![
                " .MP4 ".to_string(),
                "mp4".to_string(),
                "not a type".to_string(),
                "*.zip".to_string(),
            ],
            skip_sites: vec![
                "https://Example.ORG/download".to_string(),
                "example.org".to_string(),
                "not a host".to_string(),
            ],
        });

        assert_eq!(policy.types, vec!["mp4", "zip"]);
        assert_eq!(policy.skip_sites, vec!["example.org"]);
    }

    #[test]
    fn a_browser_hand_over_connection_count_is_clamped() {
        let state = AppState::default();
        assert_eq!(state.browser_connections(), 8, "the engine default");
        state.set_browser_connections(0);
        assert_eq!(state.browser_connections(), 1, "a count below the minimum");
        state.set_browser_connections(99);
        assert_eq!(state.browser_connections(), 32, "a count above the ceiling");
        state.set_browser_connections(16);
        assert_eq!(state.browser_connections(), 16, "a count in range");
    }

    #[test]
    fn a_proxy_is_checked_before_it_is_stored() {
        assert_eq!(
            validate_proxy(" socks5://127.0.0.1:1080 ".to_string()),
            Ok("socks5://127.0.0.1:1080".to_string())
        );
        assert_eq!(validate_proxy("  ".to_string()), Ok(String::new()));
        assert!(validate_proxy("127.0.0.1:1080".to_string()).is_err());
        assert!(validate_proxy("ftp://127.0.0.1:1080".to_string()).is_err());
        assert!(validate_proxy("http://user:secret@127.0.0.1:8080".to_string()).is_err());
    }
}
