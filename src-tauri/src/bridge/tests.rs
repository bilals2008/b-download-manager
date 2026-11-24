use std::path::{Path, PathBuf};

use super::*;

const EXTENSION_ORIGIN: &str = "moz-extension://3c1e8f2a-1";

fn request(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
    let mut all = vec![("host", "127.0.0.1:27500")];
    all.extend_from_slice(headers);
    Request::for_test(method, path, &all, b"")
}

fn extension(method: &str, path: &str, origin: &str) -> Request {
    request(
        method,
        path,
        &[("origin", origin), ("x-bdm-client", auth::CLIENT_VALUE)],
    )
}

#[test]
fn an_extension_is_recognised_without_any_code() {
    let gate = gate(&extension("GET", "/v1/downloads", EXTENSION_ORIGIN));
    assert_eq!(gate.denial, None);
    assert!(gate.extension);
    assert_eq!(gate.cors.as_deref(), Some(EXTENSION_ORIGIN));
}

#[test]
fn a_download_without_the_extension_header_is_refused() {
    let gate = gate(&request(
        "POST",
        "/v1/downloads",
        &[("origin", EXTENSION_ORIGIN)],
    ));
    assert_eq!(gate.denial.map(|denial| denial.status), Some(403));
    assert!(!gate.extension);
}

#[test]
fn the_ping_is_open_and_still_names_the_application() {
    let gate = gate(&extension("GET", "/v1/ping", EXTENSION_ORIGIN));
    assert_eq!(gate.denial, None);
    assert!(gate.extension, "the popup can say it is connected");
}

#[test]
fn a_preflight_passes_but_does_not_count_as_a_contact() {
    let gate = gate(&request(
        "OPTIONS",
        "/v1/downloads",
        &[("origin", EXTENSION_ORIGIN)],
    ));
    assert_eq!(gate.denial, None);
    assert!(
        !gate.extension,
        "a preflight is not the browser reaching us"
    );
}

#[test]
fn a_local_caller_without_an_origin_is_answered_but_not_called_connected() {
    let gate = gate(&request(
        "GET",
        "/v1/downloads",
        &[("x-bdm-client", auth::CLIENT_VALUE)],
    ));
    assert_eq!(gate.denial, None, "no origin is nothing to refuse");
    assert!(!gate.extension);
    assert_eq!(gate.cors, None, "and it gets no CORS header back");
}

#[test]
fn a_page_or_a_wrong_host_is_refused() {
    let page = gate(&extension("GET", "/v1/downloads", "https://example.org"));
    assert_eq!(page.denial.map(|denial| denial.status), Some(403));
    assert_eq!(page.cors, None, "a page origin is never echoed back");

    let rebinding = Request::for_test(
        "GET",
        "/v1/downloads",
        &[
            ("host", "evil.example:27500"),
            ("origin", EXTENSION_ORIGIN),
            ("x-bdm-client", auth::CLIENT_VALUE),
        ],
        b"",
    );
    let gate = gate(&rebinding);
    assert_eq!(gate.denial.map(|denial| denial.status), Some(403));
}

#[test]
fn an_extension_origin_is_recognised() {
    assert!(is_extension_origin("moz-extension://3c1e8f2a"));
    assert!(is_extension_origin("chrome-extension://abc"));
    assert!(is_extension_origin("safari-web-extension://3c1e8f2a-1"));
    assert!(!is_extension_origin("https://example.org"));
    assert!(!is_extension_origin("null"));
}

#[test]
fn a_page_cannot_be_a_download_source() {
    assert!(is_download_url("https://example.org/a.bin"));
    assert!(is_download_url("FTP://example.org/a.bin"));
    assert!(!is_download_url("http://"));
    assert!(!is_download_url("https:///a.bin"));
    assert!(!is_download_url("https://example.org/a b.bin"));
    assert!(!is_download_url("blob:https://example.org/abc"));
    assert!(!is_download_url("javascript:alert(1)"));
    assert!(!is_download_url("file:///C:/Windows/notepad.exe"));
}

#[test]
fn a_control_route_names_a_download_and_an_action() {
    assert_eq!(control_route("/v1/downloads/42/pause"), Some((42, "pause")));
    assert_eq!(
        control_route("/v1/downloads/42/resume"),
        Some((42, "resume"))
    );
    assert_eq!(
        control_route("/v1/downloads/42/cancel"),
        Some((42, "cancel"))
    );
    assert_eq!(control_route("/v1/downloads/42/show"), Some((42, "show")));
    assert_eq!(control_route("/v1/downloads/42"), None);
    assert_eq!(control_route("/v1/downloads/42/pause/extra"), None);
    assert_eq!(control_route("/v1/downloads/not-a-number/pause"), None);
    assert_eq!(control_route("/v1/other/42/pause"), None);
}

#[test]
fn a_media_route_names_a_job_and_an_action() {
    assert_eq!(media_route("/v1/media/7"), Some((7, None)));
    assert_eq!(
        media_route("/v1/media/7/download"),
        Some((7, Some("download")))
    );
    assert_eq!(media_route("/v1/media/7/cancel"), Some((7, Some("cancel"))));
    assert_eq!(media_route("/v1/media/7/download/x"), None);
    assert_eq!(media_route("/v1/media/nope"), None);
    assert_eq!(media_route("/v1/downloads/7"), None);
}

#[test]
fn the_body_defaults_to_a_direct_handover() {
    let request: AddRequest =
        serde_json::from_str(r#"{"url":"https://example.org/a.bin"}"#).expect("a body");
    assert_eq!(request.mode, Mode::Direct);
    assert_eq!(request.name, "");
    assert_eq!(request.referer, "");
}

#[test]
fn an_explicit_ask_is_read() {
    let request: AddRequest = serde_json::from_str(
            r#"{"url":"https://example.org/a.bin","name":"a.bin","referer":"https://example.org/","mode":"ask"}"#,
        )
        .expect("a body");
    assert_eq!(request.mode, Mode::Ask);
    assert_eq!(request.name, "a.bin");
    assert!(is_web_url(&request.referer));
}

#[test]
fn the_answers_carry_an_ok_flag() {
    assert_eq!(ok(json!({"id": 1}))["ok"], true);
    let failure = error_body("bad_request", "no");
    assert_eq!(failure["ok"], false);
    assert_eq!(failure["error"]["code"], "bad_request");
}

#[test]
fn the_ping_names_this_application() {
    let answer = ping(false);
    assert_eq!(answer["app"], APP_ID);
    assert_eq!(answer["api"], API_VERSION);
    assert_eq!(answer["port"], PORT);
    assert_eq!(answer["connected"], false);
    assert_eq!(ping(true)["connected"], true);
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "my-idm-clone-bridge-plan-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn row(id: u64, dest: &Path) -> std::sync::Arc<idm_core::downloader::Download> {
    idm_core::downloader::Download::new(
        id,
        "https://example.org/film.mp4".to_string(),
        dest.to_path_buf(),
        4,
        None,
        None,
    )
    .expect("a download")
}

#[test]
fn a_file_that_is_already_there_sends_the_handover_to_the_window() {
    let dir = temp_dir("exists");
    std::fs::write(dir.join("film.mp4"), b"old").expect("write");
    let state = AppState::default();

    match plan_direct(&state, &dir, "film.mp4") {
        DirectPlan::Taken { dest } => {
            assert_eq!(
                dest,
                dir.join("film.mp4"),
                "the question is about the real file"
            );
        }
        DirectPlan::Free => {
            panic!("a taken name is a question, not a silent rename")
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_free_name_starts_without_a_question() {
    let dir = temp_dir("free");
    let state = AppState::default();

    match plan_direct(&state, &dir, "film.mp4") {
        DirectPlan::Free => {}
        DirectPlan::Taken { .. } => panic!("nothing holds that name"),
    }
    // The free target is the one the start would write: the category folder
    // applied exactly once to the name in the default folder.
    let (target, _) =
        crate::actions::category_destination(&state, dir.join("film.mp4"), None, true);
    assert_eq!(target, dir.join("film.mp4"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_name_another_row_is_writing_is_a_question_too() {
    let dir = temp_dir("active");
    let state = AppState::default();
    state.insert(row(1, &dir.join("film.mp4")));

    // The file itself is not on disk yet — a running download keeps parts
    // elsewhere — but the name belongs to that row.
    match plan_direct(&state, &dir, "film.mp4") {
        DirectPlan::Taken { .. } => {}
        DirectPlan::Free => panic!("that name is held, but the plan says free"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_question_follows_the_category_folder() {
    let dir = temp_dir("category");
    let films = dir.join("Films");
    std::fs::create_dir_all(&films).expect("folder");
    std::fs::write(films.join("film.mp4"), b"old").expect("write");

    let state = AppState::default();
    state.set_categories(vec![crate::state::CategoryRule {
        name: "Films".to_string(),
        extensions: vec!["mp4".to_string()],
        folder: films.to_string_lossy().to_string(),
        builtin: false,
        fallback: false,
        root: false,
    }]);

    // The Downloads folder is free; the folder the file's category saves
    // to is not. The question must be about the file that would really be
    // written.
    match plan_direct(&state, &dir, "film.mp4") {
        DirectPlan::Taken { dest } => assert_eq!(dest, films.join("film.mp4")),
        DirectPlan::Free => panic!("the category folder is taken, not free"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
