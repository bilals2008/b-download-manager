//! The torrents as the download list shows them.
//!
//! A row needs a name, a size, a speed and a state. The torrent window needs
//! the peers, the piece map and the files as well. Reading the full job once a
//! second for a list would carry all of that across the bridge and draw none
//! of it, so the list asks for this small picture instead.

use serde::Serialize;

use crate::torrent::torrents;

/// One job, as the main window's list shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentSummary {
    info_hash: String,
    name: String,
    directory: String,
    downloaded: u64,
    total_length: u64,
    down_speed: u64,
    up_speed: u64,
    eta: Option<u64>,
    status: idm_torrent::JobStatus,
    error: Option<String>,
    seeders: u64,
    leechers: u64,
    files: usize,
    added_at: u64,
}

/// Every job of the client.
#[tauri::command]
pub fn torrent_summaries() -> Vec<TorrentSummary> {
    torrents()
        .client
        .jobs()
        .into_iter()
        .map(|job| TorrentSummary {
            info_hash: job.info_hash,
            name: job.name,
            directory: job.directory.display().to_string(),
            downloaded: job.downloaded,
            total_length: job.total_length,
            down_speed: job.down_speed,
            up_speed: job.up_speed,
            eta: job.eta,
            status: job.status,
            error: job.error,
            seeders: job.seeders,
            leechers: job.leechers,
            files: job.files.len(),
            added_at: job.added_at,
        })
        .collect()
}
