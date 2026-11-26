//! Image galleries: a page of pictures, into one folder.
//!
//! The page is read with the application's own fetch, so the encrypted DNS
//! and the chosen network adapter apply to it as they do to every other
//! address. Each image is an ordinary download: it can be stopped, resumed,
//! limited and categorised like any other, and the folder is named after the
//! page, with the index as the prefix of each file.
//!
//! The report says how many files were started. Sizes are not known before
//! the bytes arrive, so there is no size estimate to promise; the dialog
//! shows the count instead.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use serde::Serialize;

use idm_core::media::gallery;

use crate::bridge::dest;
use crate::state::AppState;

/// What the add dialog shows before anything starts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryInspect {
    /// Whether the page reads as a gallery of images.
    pub is_gallery: bool,
    /// The folder the files would land in.
    pub folder: String,
    /// How many images the page carries.
    pub count: usize,
    /// Whether more were found than the cap, so the dialog says so.
    pub truncated: bool,
    /// The video and the sound the page names. The Add URL dialog offers them
    /// beside the gallery: this is the one fetch of the page, so a page is
    /// never read twice for two questions.
    pub media: crate::page_media::PageMedia,
}

/// What became of the gallery download.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryReport {
    /// The folder the files went into.
    pub folder: String,
    /// How many images were started.
    pub started: usize,
    /// How many were refused before the engine was asked.
    pub refused: usize,
    /// Whether more were found than the cap, so the dialog says so.
    pub truncated: bool,
}

/// Ask the page what it holds. Nothing is downloaded.
#[tauri::command]
pub async fn gallery_inspect(
    app: AppHandle,
    url: String,
    referer: Option<String>,
) -> Result<GalleryInspect, String> {
    let _ = app;
    let page = fetch(&url, referer.as_deref()).await?;
    let found = gallery::extract(&page.text, &url);
    let media = crate::page_media::inspect(&page.text, &page.url);
    if !gallery::looks_like_gallery(&url, &page.text) || found.images.len() < 2 {
        return Ok(GalleryInspect {
            is_gallery: false,
            folder: String::new(),
            count: 0,
            truncated: false,
            media,
        });
    }
    let folder = gallery::folder_name(&found);
    Ok(GalleryInspect {
        is_gallery: true,
        folder,
        count: found.images.len(),
        truncated: found.truncated,
        media,
    })
}

/// Download every image of the gallery, one ordinary download each, into a
/// folder named after the page.
#[tauri::command]
pub async fn gallery_download(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    referer: Option<String>,
    folder: Option<String>,
) -> Result<GalleryReport, String> {
    let page = fetch(&url, referer.as_deref()).await?;
    let found = gallery::extract(&page.text, &url);
    if !gallery::looks_like_gallery(&url, &page.text) || found.images.len() < 2 {
        return Err("this page does not read as a gallery of images".to_string());
    }
    let name = gallery::folder_name(&found);
    let base = folder
        .as_deref()
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."))
                .join(&name)
        });
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("the folder could not be made: {error}"))?;

    let title = found.title.clone();
    let referer = referer
        .as_deref()
        .map(str::trim)
        .filter(|referer| !referer.is_empty())
        .map(String::from);
    let mut started = 0usize;
    let mut refused = 0usize;
    for image in &found.images {
        let file = gallery::file_name(image, &title);
        let target = dest::unique(&base, file.as_str());
        match crate::actions::start(
            &app,
            &state,
            image.url.clone(),
            target,
            crate::actions::StartOptions {
                referer: referer.clone(),
                conflict: crate::conflict::Choice::Rename,
                use_category_folder: false,
                ..crate::actions::StartOptions::default()
            },
        ) {
            Ok(_) => started += 1,
            Err(_) => refused += 1,
        }
    }
    Ok(GalleryReport {
        folder: base.to_string_lossy().to_string(),
        started,
        refused,
        truncated: found.truncated,
    })
}

/// Fetch the page, the same way every other address goes through.
async fn fetch(url: &str, referer: Option<&str>) -> Result<crate::video::probe::Fetched, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("the address is empty".to_string());
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("the address must start with http:// or https://".to_string());
    }
    crate::video::probe::fetch(url, referer.unwrap_or(""), &[], None).await
}
