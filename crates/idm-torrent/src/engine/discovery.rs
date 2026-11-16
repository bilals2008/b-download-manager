//! Where a job's peers come from when the trackers name none: the
//! distributed table, and the local network's own announcements.

use std::net::SocketAddr;
use std::sync::Arc;

use crate::dht;
use crate::peer::PeerInfo;
use crate::peerconn::PeerSession;

use super::{set_peers, Job};

/// The info hash of a job, as the twenty bytes the protocols carry.
pub(super) fn hash_of(job: &Arc<Job>) -> Option<[u8; 20]> {
    crate::util::hex_decode(&job.id).and_then(|bytes| <[u8; 20]>::try_from(bytes).ok())
}

/// Take a peer into the job's list once, and show it.
fn take(
    job: &Arc<Job>,
    addresses: &mut Vec<SocketAddr>,
    known: &mut Vec<PeerInfo>,
    session: &Option<PeerSession>,
    address: SocketAddr,
) {
    if addresses.contains(&address) {
        return;
    }
    addresses.push(address);
    known.push(PeerInfo {
        address: address.to_string(),
        ..PeerInfo::default()
    });
    if let Ok(mut state) = job.state.lock() {
        set_peers(&mut state, known, session);
    }
}

/// A torrent no tracker answered for — one that names none, or whose
/// trackers are all unreachable — finds its peers in the distributed table
/// instead, and tells the table it is here on the same port the trackers are
/// told about, so the mapping the router made for this client is the one the
/// table is reached on.
pub(super) async fn dht_fallback(
    job: &Arc<Job>,
    port: u16,
    addresses: &mut Vec<SocketAddr>,
    known: &mut Vec<PeerInfo>,
    session: &Option<PeerSession>,
) {
    let Some(table) = dht::shared() else {
        return;
    };
    let Some(hash) = hash_of(job) else {
        return;
    };
    for address in table.peers(hash).await {
        take(job, addresses, known, session, address);
    }
    table.announce(hash, port).await;
}

/// Say on the local network that this job's torrent is held here.
pub(super) fn lpd_announce(job: &Arc<Job>) {
    let Some(hash) = hash_of(job) else {
        return;
    };
    if let Ok(held) = job.lpd.lock() {
        if let Some(lpd) = held.as_ref() {
            lpd.announce(hash);
        }
    }
}

/// Stop saying so, and forget the peers the announcement brought.
pub(super) fn lpd_forget(job: &Arc<Job>) {
    let Some(hash) = hash_of(job) else {
        return;
    };
    if let Ok(held) = job.lpd.lock() {
        if let Some(lpd) = held.as_ref() {
            lpd.forget(hash);
        }
    }
}

/// Take the peers the local network announced for this torrent.
pub(super) async fn lpd_peers(
    job: &Arc<Job>,
    addresses: &mut Vec<SocketAddr>,
    known: &mut Vec<PeerInfo>,
    session: &Option<PeerSession>,
) {
    let lpd = job.lpd.lock().ok().and_then(|held| held.clone());
    let Some(lpd) = lpd else {
        return;
    };
    let Some(hash) = hash_of(job) else {
        return;
    };
    for address in lpd.peers(hash).await {
        take(job, addresses, known, session, address);
    }
}