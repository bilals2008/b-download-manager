//! Why a torrent could not be read, announced or written.

use std::fmt;

/// Why a torrent could not be read, announced or written.
#[derive(Debug)]
pub enum TorrentError {
    /// The bytes are not a torrent, or the link is not a magnet link.
    Malformed(String),
    /// The torrent uses something this crate does not read.
    Unsupported(String),
    /// The name would leave the folder the files go into.
    Unsafe(String),
    /// The tracker could not be reached, or answered with something else.
    Network(String),
    /// Something went wrong on the disk.
    Io(std::io::Error),
    /// The job stopped with an error.
    Failed(String),
}

impl fmt::Display for TorrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TorrentError::Malformed(why) => write!(f, "the torrent is not valid: {why}"),
            TorrentError::Unsupported(why) => write!(f, "this torrent is not supported: {why}"),
            TorrentError::Unsafe(why) => write!(f, "the name would leave the folder: {why}"),
            TorrentError::Network(why) => write!(f, "the tracker could not be reached: {why}"),
            TorrentError::Io(e) => write!(f, "the disk could not be used: {e}"),
            TorrentError::Failed(why) => write!(f, "the job failed: {why}"),
        }
    }
}

impl std::error::Error for TorrentError {}

impl From<std::io::Error> for TorrentError {
    fn from(e: std::io::Error) -> Self {
        TorrentError::Io(e)
    }
}
