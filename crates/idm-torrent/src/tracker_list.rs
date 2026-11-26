//! The trackers a reader adds to a job, and the ones they take away.

use crate::engine::Source;

/// The trackers the reader added to a job, and the ones they took away.
///
/// The torrents a magnet or a `.torrent` names are the ones it came with; a
/// reader who knows a tracker it does not name can add it, and one that is
/// dead can be dropped, without the job starting over.
#[derive(Default)]
pub(crate) struct TrackerEdits {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// The trackers of a job, in the order they are asked: the ones the torrent
/// came with, then the ones the reader added, without the ones they dropped.
/// The same address written twice is asked once, whatever its case.
pub(crate) fn trackers_of(source: &Source, edits: &TrackerEdits) -> Vec<String> {
    let base: &[String] = match source {
        Source::Magnet(magnet) => &magnet.trackers,
        Source::Metainfo(metainfo) => &metainfo.trackers,
    };
    let mut urls: Vec<String> = Vec::new();
    for url in base.iter().chain(edits.added.iter()) {
        let url = url.trim();
        if url.is_empty() {
            continue;
        }
        if edits
            .removed
            .iter()
            .any(|removed| removed.eq_ignore_ascii_case(url))
        {
            continue;
        }
        if urls.iter().any(|held| held.eq_ignore_ascii_case(url)) {
            continue;
        }
        urls.push(url.to_owned());
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magnet::MagnetInfo;

    fn source(trackers: &[&str]) -> Source {
        Source::Magnet(MagnetInfo {
            info_hash: "a".repeat(40),
            name: None,
            trackers: trackers.iter().map(|url| url.to_string()).collect(),
        })
    }

    #[test]
    fn added_trackers_come_after_the_ones_the_torrent_names() {
        let mut edits = TrackerEdits::default();
        edits.added.push("udp://new.example:6969".into());
        let urls = trackers_of(&source(&["udp://old.example:80"]), &edits);
        assert_eq!(
            urls,
            vec!["udp://old.example:80", "udp://new.example:6969"]
        );
    }

    #[test]
    fn a_removed_tracker_is_gone_even_when_it_is_written_again_in_another_case() {
        let mut edits = TrackerEdits::default();
        edits.added.push("UDP://One.Example:80".into());
        edits.removed.push("udp://one.example:80".into());
        assert!(trackers_of(&source(&["udp://one.example:80"]), &edits).is_empty());
    }

    #[test]
    fn the_same_address_written_twice_is_asked_once() {
        let mut edits = TrackerEdits::default();
        edits.added.push(" udp://same.example:80 ".into());
        let urls = trackers_of(&source(&["udp://same.example:80"]), &edits);
        assert_eq!(urls, vec!["udp://same.example:80"]);
    }
}
