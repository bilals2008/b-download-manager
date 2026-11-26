//! A page pasted into the app, opened in the quality panel.
//!
//! The browser registers a manifest it saw on a page; a link pasted into Add
//! URL has no manifest yet. This makes the job the panel reads and starts
//! resolving it in the background, so the panel opens at once and says it is
//! reading the page instead of waiting for a window.
//!
//! A playlist or a media file pasted on its own is parsed as what it is. Any
//! other address is a page: the video helper reads it for the sites it knows,
//! and the in-house YouTube path answers when no helper is installed.

use tauri::{AppHandle, Manager};

use crate::bridge::dest;
use crate::state::AppState;

use super::helpers::infer_kind;
use super::parsing::parse_job;
use crate::video::{host_of, window, MediaJob, MediaKindHint};

/// Open a page or a media address in the quality panel, and answer its job.
#[tauri::command]
pub async fn page_media_open(
    app: AppHandle,
    url: String,
    referer: Option<String>,
    title: Option<String>,
) -> Result<u64, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("The address must start with http:// or https://.".to_string());
    }
    let state = app.state::<AppState>();

    // The same address opened twice is the same list: the panel is brought
    // forward instead of being filled a second time.
    if let Some(existing) = state.media_job_for_manifest(url) {
        window::show(&app, existing);
        return Ok(existing);
    }

    // The address decides what the panel reads. A manifest is parsed as one;
    // anything else is a page the helper resolves.
    let kind = match infer_kind(MediaKindHint::Auto, url) {
        MediaKindHint::Auto => MediaKindHint::Page,
        kind => kind,
    };
    let preferred = state.video_quality(&host_of(url, url));
    let title = title
        .as_deref()
        .and_then(dest::sanitize)
        .unwrap_or_default();

    let id = state.next_media_id();
    state.insert_media_job(MediaJob::new(
        id,
        url.to_string(),
        url.to_string(),
        kind,
        title,
        referer.unwrap_or_default(),
        Vec::new(),
        preferred,
    ));

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        parse_job(handle, id).await;
    });
    window::show(&app, id);
    Ok(id)
}
