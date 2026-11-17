//! Pacing the connections of a download.
//!
//! Two questions are asked about the connections that are running: which one
//! has stopped making progress, and which one is holding the file back. Both
//! are decided here, from the bytes each range has taken, so the supervisor in
//! the downloader stays a caller and the rules stay testable without a
//! network.
//!
//! A rate is measured over a moving window rather than over the whole run: a
//! connection that ran fast for a minute and then stalled is stalled now, and
//! its average would hide that.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::segmenter::{Segment, MIN_SPLIT};

/// How much of a connection's history a rate covers.
pub const WINDOW: Duration = Duration::from_secs(4);

/// Below this share of the median rate a connection is called stalled.
pub const STALLED_SHARE: f64 = 0.25;

/// How many moving connections it takes before one can be called stalled.
///
/// The median of two connections says nothing: one of them is always the
/// slower one.
pub const MIN_MOVING: usize = 2;

/// One connection's bytes and the moment they were seen.
#[derive(Debug, Clone, Copy)]
struct Beat {
    /// Bytes at the start of the window.
    window_bytes: u64,
    /// When the window started.
    window_at: Instant,
    /// Bytes and moment of the newest sample.
    bytes: u64,
    at: Instant,
}

/// What each connection of a download has taken, and when.
#[derive(Debug, Default)]
pub struct Watch {
    beats: HashMap<u32, Beat>,
}

impl Watch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note how far a connection has got.
    pub fn record(&mut self, id: u32, downloaded: u64) {
        self.record_at(id, downloaded, Instant::now());
    }

    /// Note how far a connection had got at a moment.
    ///
    /// The moment is passed in so the rules can be tested without waiting for
    /// a clock: a test that measures four seconds of traffic should not take
    /// four seconds.
    pub fn record_at(&mut self, id: u32, downloaded: u64, now: Instant) {
        match self.beats.get_mut(&id) {
            Some(beat) => {
                beat.bytes = downloaded;
                beat.at = now;
            }
            None => {
                self.beats.insert(
                    id,
                    Beat {
                        window_bytes: downloaded,
                        window_at: now,
                        bytes: downloaded,
                        at: now,
                    },
                );
            }
        }
    }

    /// Forget a connection that is no longer running.
    pub fn forget(&mut self, id: u32) {
        self.beats.remove(&id);
    }

    /// Forget everything, at the start of a run.
    pub fn clear(&mut self) {
        self.beats.clear();
    }

    /// The bytes per second of every connection, over the window.
    ///
    /// A window that has run its length slides to the newest sample, so a
    /// connection that stalls later is measured on the stall and not on the
    /// whole of its life.
    pub fn rates(&mut self, now: Instant) -> HashMap<u32, f64> {
        self.beats
            .iter_mut()
            .filter_map(|(id, beat)| {
                let span = now.saturating_duration_since(beat.window_at);
                if span.is_zero() {
                    return None;
                }
                let taken = beat.bytes.saturating_sub(beat.window_bytes);
                let rate = taken as f64 / span.as_secs_f64();
                // The window is rolled after the rate is read, so the read
                // covers the samples that were in it. A connection that stops
                // taking bytes therefore reads as zero over the next window
                // instead of disappearing from the list.
                if span >= WINDOW {
                    beat.window_bytes = beat.bytes;
                    beat.window_at = now;
                }
                Some((*id, rate))
            })
            .collect()
    }
}

/// The median of a list of rates.
fn median(mut rates: Vec<f64>) -> Option<f64> {
    if rates.is_empty() {
        return None;
    }
    rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = rates.len() / 2;
    Some(if rates.len().is_multiple_of(2) {
        (rates[middle - 1] + rates[middle]) / 2.0
    } else {
        rates[middle]
    })
}

/// The connection that has stopped moving, when there is one.
///
/// A range is a candidate when it still has a worthwhile tail (`MIN_SPLIT`),
/// when at least [`MIN_MOVING`] other ranges are moving, and when its own rate
/// is below [`STALLED_SHARE`] of their median. A range that has not been seen
/// moving at all is the strongest candidate of all.
pub fn stalled(segments: &[Segment], rates: &HashMap<u32, f64>) -> Option<u32> {
    let unfinished: Vec<&Segment> = segments
        .iter()
        .filter(|segment| !segment.done() && segment.end != crate::segmenter::UNKNOWN_END)
        .collect();

    let mut best: Option<(u32, f64)> = None;
    for candidate in &unfinished {
        let left = candidate.len().saturating_sub(candidate.downloaded);
        if left < MIN_SPLIT {
            continue;
        }
        let others: Vec<f64> = unfinished
            .iter()
            .filter(|segment| segment.id != candidate.id)
            .filter_map(|segment| rates.get(&segment.id).copied())
            .filter(|rate| *rate > 0.0)
            .collect();
        if others.len() < MIN_MOVING {
            continue;
        }
        let Some(benchmark) = median(others) else {
            continue;
        };
        let rate = rates.get(&candidate.id).copied().unwrap_or(0.0);
        if rate <= benchmark * STALLED_SHARE && (best.is_none() || rate < best.map(|b| b.1).unwrap_or(f64::MAX)) {
            best = Some((candidate.id, rate));
        }
    }

    best.map(|(id, _)| id)
}

/// The connection that is moving least, when one of them can be improved.
///
/// This is the range worth giving another connection when one comes free: it
/// has a worthwhile tail left and lags the others. `None` when every range is
/// moving at much the same rate, because splitting then buys nothing.
pub fn lagging(segments: &[Segment], rates: &HashMap<u32, f64>) -> Option<u32> {
    let moving: Vec<&Segment> = segments
        .iter()
        .filter(|segment| !segment.done())
        .filter(|segment| segment.end != crate::segmenter::UNKNOWN_END)
        .filter(|segment| segment.len().saturating_sub(segment.downloaded) >= MIN_SPLIT)
        .collect();
    if moving.len() < 2 {
        return None;
    }

    let mut slowest: Option<(&Segment, f64)> = None;
    for segment in &moving {
        let rate = rates.get(&segment.id).copied().unwrap_or(0.0);
        if slowest.is_none() || rate < slowest.map(|(_, r)| r).unwrap_or(f64::MAX) {
            slowest = Some((segment, rate));
        }
    }
    let (segment, rate) = slowest?;
    let others: Vec<f64> = moving
        .iter()
        .filter(|other| other.id != segment.id)
        .filter_map(|other| rates.get(&other.id).copied())
        .collect();
    let benchmark = median(others)?;
    if benchmark <= 0.0 || rate >= benchmark * 0.75 {
        return None;
    }
    Some(segment.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmenter::{SegmentStatus, UNKNOWN_END};
    use std::path::PathBuf;

    fn segment(id: u32, start: u64, end: u64, downloaded: u64) -> Segment {
        Segment {
            id,
            start,
            end,
            downloaded,
            part: PathBuf::from(format!("part{id}")),
            status: SegmentStatus::Receiving,
        }
    }

    fn rates(pairs: &[(u32, f64)]) -> HashMap<u32, f64> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn the_median_of_an_odd_and_an_even_list() {
        assert_eq!(median(vec![1.0, 3.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0]), Some(2.5));
        assert_eq!(median(Vec::new()), None);
    }

    #[test]
    fn a_stopped_connection_is_found_among_moving_ones() {
        let segments = vec![
            segment(0, 0, 8 * MIN_SPLIT, MIN_SPLIT),
            segment(1, 8 * MIN_SPLIT, 16 * MIN_SPLIT, MIN_SPLIT),
            segment(2, 16 * MIN_SPLIT, 24 * MIN_SPLIT, MIN_SPLIT),
        ];
        let moving = rates(&[(0, 1_000_000.0), (1, 1_200_000.0), (2, 0.0)]);
        assert_eq!(stalled(&segments, &moving), Some(2));
    }

    #[test]
    fn two_connections_are_never_enough_to_call_one_stalled() {
        let segments = vec![
            segment(0, 0, 8 * MIN_SPLIT, MIN_SPLIT),
            segment(1, 8 * MIN_SPLIT, 16 * MIN_SPLIT, MIN_SPLIT),
        ];
        let moving = rates(&[(0, 1_000_000.0), (1, 0.0)]);
        assert_eq!(stalled(&segments, &moving), None);
    }

    #[test]
    fn a_connection_with_nothing_worth_moving_is_left_alone() {
        let segments = vec![
            segment(0, 0, 8 * MIN_SPLIT, MIN_SPLIT),
            segment(1, 8 * MIN_SPLIT, 16 * MIN_SPLIT, MIN_SPLIT),
            segment(2, 16 * MIN_SPLIT, 16 * MIN_SPLIT + 1024, 0),
        ];
        let moving = rates(&[(0, 1_000_000.0), (1, 1_000_000.0), (2, 0.0)]);
        assert_eq!(stalled(&segments, &moving), None);
    }

    #[test]
    fn a_range_whose_length_is_unknown_is_never_split() {
        let segments = vec![
            segment(0, 0, 8 * MIN_SPLIT, MIN_SPLIT),
            segment(1, 8 * MIN_SPLIT, 16 * MIN_SPLIT, MIN_SPLIT),
            segment(2, 16 * MIN_SPLIT, UNKNOWN_END, 0),
        ];
        let moving = rates(&[(0, 1_000_000.0), (1, 1_000_000.0), (2, 0.0)]);
        assert_eq!(stalled(&segments, &moving), None);
    }

    #[test]
    fn the_least_moving_connection_is_the_one_to_give_another() {
        let segments = vec![
            segment(0, 0, 8 * MIN_SPLIT, MIN_SPLIT),
            segment(1, 8 * MIN_SPLIT, 16 * MIN_SPLIT, MIN_SPLIT),
        ];
        let moving = rates(&[(0, 2_000_000.0), (1, 100_000.0)]);
        assert_eq!(lagging(&segments, &moving), Some(1));
    }

    #[test]
    fn connections_that_move_alike_are_not_rebalanced() {
        let segments = vec![
            segment(0, 0, 8 * MIN_SPLIT, MIN_SPLIT),
            segment(1, 8 * MIN_SPLIT, 16 * MIN_SPLIT, MIN_SPLIT),
        ];
        let moving = rates(&[(0, 1_000_000.0), (1, 900_000.0)]);
        assert_eq!(lagging(&segments, &moving), None);
    }

    #[test]
    fn a_window_measures_the_bytes_it_saw() {
        let mut watch = Watch::new();
        let start = Instant::now();
        watch.record_at(0, 0, start);
        watch.record_at(0, 1000, start + Duration::from_secs(2));
        let rates = watch.rates(start + Duration::from_secs(2));
        assert_eq!(rates.get(&0).copied(), Some(500.0));
    }

    #[test]
    fn a_connection_that_stops_reads_as_zero_after_its_window() {
        let mut watch = Watch::new();
        let start = Instant::now();
        watch.record_at(0, 0, start);
        watch.record_at(0, 1000, start + Duration::from_secs(2));
        // Nothing has arrived since: the window that just ended took 1000
        // bytes over eight seconds, and the window is then rolled.
        let rates = watch.rates(start + Duration::from_secs(8));
        assert_eq!(rates.get(&0).copied(), Some(125.0));
        // Now the window is the quiet one, so the rate is nothing at all.
        let rates = watch.rates(start + Duration::from_secs(9));
        assert_eq!(rates.get(&0).copied(), Some(0.0));
    }
}
