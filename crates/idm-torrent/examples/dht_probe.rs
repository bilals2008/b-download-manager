//! An honest check of the distributed table against the real network.
//!
//! It takes a `.torrent` file, strips every tracker from it, and asks the
//! table for the peers of the info hash — first for the file list, the way
//! the add dialog does, then as a running job, the way the engine does. It
//! prints the numbers it actually saw: the peers the table named, the bytes
//! that came in. It claims nothing it did not see.
//!
//!     cargo run -p idm-torrent --example dht_probe -- path/to/a.torrent [seconds]

use std::time::{Duration, Instant};

use idm_torrent::{lookup, JobRequest, JobSource, JobStatus, TorrentClient};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: dht_probe <file.torrent> [seconds]");
    let seconds: u64 = std::env::args()
        .nth(2)
        .map(|text| text.parse().expect("a number of seconds"))
        .unwrap_or(90);
    let bytes = std::fs::read(&path).expect("the torrent file");
    let metainfo = idm_torrent::parse_torrent(&bytes).expect("the torrent");
    // The magnet carries the info hash and nothing else: no `tr`, so every
    // peer that answers is one the table found.
    let link = format!("magnet:?xt=urn:btih:{}", metainfo.info_hash);
    println!(
        "probing {} ({}) with no trackers",
        metainfo.name, metainfo.info_hash
    );

    let started = Instant::now();
    match lookup(&JobSource::Magnet(link.clone())).await {
        Ok(found) => println!(
            "lookup: the file list came from a table peer: {} files, {} bytes, in {:.1}s",
            found.files.len(),
            found.total_length,
            started.elapsed().as_secs_f64()
        ),
        Err(error) => println!("lookup: no file list: {error}"),
    }

    let directory = std::env::temp_dir().join(format!("dht-probe-{}", std::process::id()));
    let client = TorrentClient::new();
    let id = client
        .add_job(JobRequest::magnet(&link, directory.clone()))
        .await
        .expect("the job started");
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut peak_peers = 0usize;
    let mut downloaded = 0u64;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let Some(job) = client.job(&id) else { break };
        peak_peers = peak_peers.max(job.peers.len());
        downloaded = job.downloaded;
        println!(
            "t+{:.0}s: {:?}, {} peers known, {} connected, {} bytes down, {} B/s",
            started.elapsed().as_secs_f64(),
            job.status,
            job.peers.len(),
            job.peers.iter().filter(|peer| peer.connected).count(),
            job.downloaded,
            job.down_speed
        );
        if matches!(job.status, JobStatus::Completed | JobStatus::Failed(_)) {
            break;
        }
    }
    println!(
        "result: {downloaded} bytes downloaded, at most {peak_peers} peers known at once"
    );
    let _ = client.remove(&id, true).await;
    let _ = std::fs::remove_dir_all(&directory);
    if downloaded == 0 {
        std::process::exit(1);
    }
}