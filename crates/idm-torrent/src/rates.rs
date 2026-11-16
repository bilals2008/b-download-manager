//! The speeds of a job: how many bytes a second come in and go out, how
//! long the rest will take, and how fast each peer goes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::engine::Job;

/// Keep the speeds of the job up to date. Every second the bytes that came
/// in and went out since the last time are counted, the peers are counted
/// on their own, and the trackers are told how long it is until they are
/// asked again.
pub(crate) async fn tick(job: Arc<Job>) {
    let mut ticker = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let mut last = Instant::now();
    let mut last_downloaded = 0u64;
    let mut last_uploaded = 0u64;
    let mut peers: HashMap<String, (u64, u64)> = HashMap::new();

    loop {
        ticker.tick().await;
        let Ok(mut state) = job.state.lock() else {
            return;
        };
        let now = Instant::now();
        let seconds = now.duration_since(last).as_secs_f64().max(0.001);
        last = now;

        if job.control.paused() {
            // A job that is paused goes nowhere: the speeds are zero.
            state.down_speed = 0;
            state.up_speed = 0;
            state.eta = None;
            last_downloaded = state.downloaded;
            last_uploaded = state.uploaded;
            continue;
        }

        let downloaded = state.downloaded;
        let uploaded = state.uploaded;
        state.down_speed = rate(downloaded.saturating_sub(last_downloaded), seconds);
        state.up_speed = rate(uploaded.saturating_sub(last_uploaded), seconds);
        last_downloaded = downloaded;
        last_uploaded = uploaded;
        state.eta = match (state.down_speed, state.metainfo.as_ref()) {
            (speed, Some(metainfo)) if speed > 0 => {
                Some(metainfo.total_length.saturating_sub(downloaded) / speed)
            }
            _ => None,
        };

        // Every peer is counted on its own: the bytes it sent and the bytes
        // it took since the last time.
        let mut listed = state.peers.clone();
        for peer in listed.iter_mut() {
            let (down, up) = peers
                .get(&peer.address)
                .copied()
                .unwrap_or((peer.downloaded, peer.uploaded));
            peer.down_speed = rate(peer.downloaded.saturating_sub(down), seconds);
            peer.up_speed = rate(peer.uploaded.saturating_sub(up), seconds);
            peers.insert(peer.address.clone(), (peer.downloaded, peer.uploaded));
        }
        state.peers = listed;

        // The trackers are asked again in a while.
        for tracker in state.trackers.iter_mut() {
            if let Some(next) = tracker.next_in {
                tracker.next_in = Some(next.saturating_sub(1));
            }
        }
    }
}

/// How many bytes a second that is.
fn rate(bytes: u64, seconds: f64) -> u64 {
    (bytes as f64 / seconds).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_number_of_bytes_over_a_known_second_is_a_rate() {
        assert_eq!(rate(1024, 1.0), 1024);
        assert_eq!(rate(2048, 2.0), 1024);
        assert_eq!(rate(0, 1.0), 0);
        // Half a byte a second is rounded to the nearest byte.
        assert_eq!(rate(1, 2.0), 1);
    }
}
