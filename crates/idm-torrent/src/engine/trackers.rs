//! The trackers of a job, as the rows the window shows: which one answered,
//! what it counted, and which one is quiet.

use std::sync::Arc;

use crate::error::TorrentError;
use crate::session::{TrackerHealth, TrackerState};
use crate::tracker;

use super::{Job, JobState};

/// The trackers of a job, as rows that have not answered yet.
pub(super) fn list_trackers(job: &Arc<Job>) {
    let urls = job.trackers();
    if let Ok(mut state) = job.state.lock() {
        state.trackers = urls
            .into_iter()
            .map(|url| TrackerState {
                url,
                state: TrackerHealth::Working,
                message: None,
                peers: 0,
                seeders: 0,
                leechers: 0,
                next_in: None,
            })
            .collect();
    }
}

/// What a tracker's row says after an announce: the peers it named, what it
/// counts, or the words it failed with, and when it is asked again.
pub(super) fn note_tracker(
    state: &mut JobState,
    url: &str,
    answer: &Result<tracker::AnnounceResult, TorrentError>,
    next_in: u64,
) {
    if !state.trackers.iter().any(|tracker| tracker.url == url) {
        state.trackers.push(TrackerState {
            url: url.to_owned(),
            state: TrackerHealth::Working,
            message: None,
            peers: 0,
            seeders: 0,
            leechers: 0,
            next_in: None,
        });
    }
    let Some(entry) = state.trackers.iter_mut().find(|tracker| tracker.url == url) else {
        return;
    };
    match answer {
        Ok(result) => {
            entry.state = TrackerHealth::Ok;
            entry.message = None;
            entry.peers = result.peers.len() as u64;
            entry.seeders = result.complete;
            entry.leechers = result.incomplete;
        }
        Err(error) => {
            entry.state = TrackerHealth::Error;
            entry.message = Some(error.to_string());
            entry.peers = 0;
            entry.seeders = 0;
            entry.leechers = 0;
        }
    }
    entry.next_in = Some(next_in);
}
