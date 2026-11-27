//! The trackers a reader adds to a job, and the ones they take away.
//!
//! The engine reads the list every time it announces, so a change made here
//! reaches a running job without a restart; the row the window shows is
//! brought in line at the same moment, keeping what each tracker already
//! answered.

use std::collections::HashMap;

use crate::error::TorrentError;
use crate::session::{TrackerHealth, TrackerState};
use crate::tracker_list::{trackers_of, TrackerEdits};

use super::{Entry, TorrentClient};

/// Which way a change to the trackers goes.
#[derive(Clone, Copy)]
enum TrackerEdit {
    Add,
    Remove,
}

impl TorrentClient {
    /// Add trackers to a job, on top of the ones the torrent came with. The
    /// job announces to them in the next round, so a torrent with a dead
    /// tracker list gets new life without being added again.
    pub fn add_trackers(&self, id: &str, urls: &[String]) -> Result<(), TorrentError> {
        self.edit_trackers(id, urls, TrackerEdit::Add)
    }

    /// Drop trackers from a job. The torrent's own list is not touched on
    /// disk, only this run of it.
    pub fn remove_trackers(&self, id: &str, urls: &[String]) -> Result<(), TorrentError> {
        self.edit_trackers(id, urls, TrackerEdit::Remove)
    }

    /// Ask the job's trackers again now, instead of at the interval they set.
    pub fn announce_now(&self, id: &str) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        entry.control.request_announce();
        Ok(())
    }

    fn edit_trackers(
        &self,
        id: &str,
        urls: &[String],
        edit: TrackerEdit,
    ) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        {
            let mut edits = entry
                .edits
                .lock()
                .map_err(|_| TorrentError::Failed("the trackers are not readable".into()))?;
            for url in urls {
                let url = url.trim();
                if url.is_empty() {
                    continue;
                }
                match edit {
                    TrackerEdit::Add => {
                        edits.removed.retain(|held| !held.eq_ignore_ascii_case(url));
                        if !edits
                            .added
                            .iter()
                            .any(|held| held.eq_ignore_ascii_case(url))
                        {
                            edits.added.push(url.to_owned());
                        }
                    }
                    TrackerEdit::Remove => {
                        edits.added.retain(|held| !held.eq_ignore_ascii_case(url));
                        if !edits
                            .removed
                            .iter()
                            .any(|held| held.eq_ignore_ascii_case(url))
                        {
                            edits.removed.push(url.to_owned());
                        }
                    }
                }
            }
        }
        sync_tracker_rows(entry);
        entry.control.request_announce();
        Ok(())
    }
}

/// Bring the job's tracker rows in line with the reader's edits, keeping what
/// each tracker already answered.
fn sync_tracker_rows(entry: &Entry) {
    let edits = entry
        .edits
        .lock()
        .map(|edits| TrackerEdits {
            added: edits.added.clone(),
            removed: edits.removed.clone(),
        })
        .unwrap_or_default();
    let urls = trackers_of(&entry.source, &edits);
    let Ok(mut state) = entry.state.lock() else {
        return;
    };
    let mut known: HashMap<String, TrackerState> = state
        .trackers
        .drain(..)
        .map(|tracker| (tracker.url.to_ascii_lowercase(), tracker))
        .collect();
    state.trackers = urls
        .into_iter()
        .map(|url| {
            known.remove(&url.to_ascii_lowercase()).unwrap_or(TrackerState {
                url,
                state: TrackerHealth::Working,
                message: None,
                peers: 0,
                seeders: 0,
                leechers: 0,
                next_in: None,
            })
        })
        .collect();
}
