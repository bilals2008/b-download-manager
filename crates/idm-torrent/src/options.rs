//! What the reader allows the torrent client: the proxy its trackers leave
//! through, the local discovery it may speak, and the port it asks the
//! router to open.
//!
//! The crate's own defaults are quiet — no proxy, no local announcements, no
//! router changes — so a job started without options never speaks to anything
//! but its trackers and peers. The application hands over the reader's
//! choices, and they are read again every round of a running job, so a
//! change is felt without a restart.

/// The choices the torrent client runs under.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TorrentOptions {
    /// The proxy tracker announces leave through, as an address. `None` is
    /// a direct connection. Peer connections and UDP trackers do not use
    /// it; the settings say so plainly.
    pub proxy: Option<String>,
    /// Whether the client announces its torrents on the local network
    /// (BEP 14) and takes the peers that answer.
    pub lpd: bool,
    /// Whether the client asks the router to open the torrent port
    /// (UPnP, or NAT-PMP when no IGD answers).
    pub port_mapping: bool,
    /// The port the trackers are told about and the router is asked to
    /// open. Zero is the engine's own port.
    pub port: u16,
}

impl TorrentOptions {
    /// The port this client speaks of: the reader's, or the engine's own.
    pub fn listen_port(&self) -> u16 {
        if self.port == 0 {
            crate::dht::PORT
        } else {
            self.port
        }
    }
}