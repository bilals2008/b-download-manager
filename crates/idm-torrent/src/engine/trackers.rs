//! The trackers of a job, as the rows the window shows: which one answered,
//! what it counted, and which one is quiet.

use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;

use crate::error::TorrentError;
use crate::peer::PeerInfo;
use crate::peerconn::PeerSession;
use crate::session::{TrackerHealth, TrackerState};
use crate::tracker;

use super::{set_peers, Job, JobState};

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

/// Ask every tracker of the job at the same time and take what each answers.
///
/// Asked one after the other, a tracker that is down spends the whole announce
/// budget and the ones behind it are never reached — a torrent with eight
/// trackers could end up with none of them heard from. Every answer becomes
/// that tracker's row, and every peer it named is added once.
///
/// The answer says whether any tracker replied and the shortest interval one
/// asked for, which is how soon a swarm's new peers are heard about.
pub(super) async fn announce_all(
    job: &Arc<Job>,
    peer_id: &str,
    port: u16,
    left: u64,
    proxy: Option<&str>,
    addresses: &mut Vec<SocketAddr>,
    known: &mut Vec<PeerInfo>,
    session: &Option<PeerSession>,
    interval: Duration,
) -> (bool, Duration) {
    let mut asked = interval;
    let mut answered = false;
    let mut asking = tokio::task::JoinSet::new();
    for url in job.trackers() {
        let hash = job.id.clone();
        let peer = peer_id.to_string();
        let proxy = proxy.map(str::to_string);
        asking.spawn(async move {
            let answer = tracker::announce(
                &url,
                &hash,
                &peer,
                port,
                "started",
                left,
                proxy.as_deref(),
            )
            .await;
            (url, answer)
        });
    }
    while let Some(joined) = asking.join_next().await {
        let Ok((url, answer)) = joined else {
            continue;
        };
        if let Ok(result) = &answer {
            answered = true;
            for address in &result.peers {
                if !addresses.contains(address) {
                    addresses.push(*address);
                    known.push(PeerInfo {
                        address: address.to_string(),
                        ..PeerInfo::default()
                    });
                }
            }
            if result.interval > 0 {
                let wanted = Duration::from_secs(result.interval.clamp(30, 3600));
                if wanted < asked {
                    asked = wanted;
                }
            }
        }
        if let Ok(mut state) = job.state.lock() {
            note_tracker(&mut state, &url, &answer, asked.as_secs());
            state.seeders = state.trackers.iter().map(|t| t.seeders).max().unwrap_or(0);
            state.leechers = state.trackers.iter().map(|t| t.leechers).max().unwrap_or(0);
            set_peers(&mut state, known, session);
        }
    }
    (answered, asked)
}
