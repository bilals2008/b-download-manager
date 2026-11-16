//! BitTorrent support for the download engine.
//!
//! The crate reads a magnet link or a `.torrent` file, tells the trackers
//! who is asking, talks to the peers and writes the pieces into the folder
//! the application handed over. It never decides where a download ends up:
//! the files are written as part files in the folder it was given, and the
//! application moves them when the job is finished.
//!
//! The pieces are cut in order, so a film can be played while it is still
//! coming in. Every piece is checked against the hash the torrent carries
//! before it is written.
//!
//! Nothing here knows about windows: the application drives the jobs and
//! reads their state back.

mod bencode;
mod dht;
mod engine;
mod error;
mod lookup;
mod lpd;
mod magnet;
mod metainfo;
mod options;
mod peer;
mod peerconn;
mod pieces;
mod portmap;
mod rates;
mod safe;
mod session;
mod storage;
mod tracker;
mod util;

pub use crate::bencode::{bencode_decode, bencode_encode, BencodeValue};
pub use crate::dht::{set_nodes, Dht};
pub use crate::error::TorrentError;
pub use crate::lookup::{lookup, lookup_with};
pub use crate::magnet::{parse_magnet, MagnetInfo};
pub use crate::metainfo::{parse_torrent, Metainfo, TorrentFile};
pub use crate::options::TorrentOptions;
pub use crate::peer::PeerInfo;
pub use crate::pieces::{file_selection, PieceMap};
pub use crate::portmap::Status as PortMappingStatus;
pub use crate::safe::safe_path;
pub use crate::session::{
    JobFile, JobRequest, JobSource, JobStatus, TorrentClient, TorrentJob, TrackerHealth,
    TrackerState,
};
pub use crate::tracker::{announce, AnnounceResult};
