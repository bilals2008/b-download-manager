//! An opt-in proof of a magnet download against a real swarm.
//!
//! It takes a `.torrent` address (or a local file), derives the magnet from
//! its info hash and trackers, and runs a real job in a temp folder for a
//! bounded time — printing the status, the peers and the verified bytes as
//! they come. It is never part of the application or of CI: an empty swarm
//! is an honest result, not a failure. Only a job that crashes is one.
//!
//!     cargo run -p idm-torrent --example magnet_probe -- [url-or-file] [seconds]

use std::time::{Duration, Instant};

use idm_torrent::{parse_torrent, JobRequest, JobSource, JobStatus, TorrentClient};

/// Where a current, legal, well-seeded torrent is found when none is given.
const DEBIAN_LIST: &str = "https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let source = std::env::args().nth(1);
    let seconds: u64 = std::env::args()
        .nth(2)
        .map(|text| text.parse().expect("a number of seconds"))
        .unwrap_or(120);

    let bytes = match read_torrent(source.as_deref()).await {
        Ok(bytes) => bytes,
        Err(error) => {
            println!("the torrent could not be read: {error}");
            std::process::exit(1);
        }
    };
    let metainfo = match parse_torrent(&bytes) {
        Ok(metainfo) => metainfo,
        Err(error) => {
            println!("the torrent is not one this engine reads: {error}");
            std::process::exit(1);
        }
    };
    let mut link = format!(
        "magnet:?xt=urn:btih:{}&dn={}",
        metainfo.info_hash,
        percent_encode(&metainfo.name)
    );
    for tracker in &metainfo.trackers {
        link.push_str(&format!("&tr={}", percent_encode(tracker)));
    }
    println!(
        "probing {} ({} bytes, {} trackers)",
        metainfo.name,
        metainfo.total_length,
        metainfo.trackers.len()
    );

    let directory = std::env::temp_dir().join(format!("magnet-probe-{}", std::process::id()));
    let client = TorrentClient::new();
    let id = match client
        .add_job(JobRequest {
            source: JobSource::Magnet(link),
            directory: directory.clone(),
            wanted: Vec::new(),
            start_paused: false,
            sequential: false,
        })
        .await
    {
        Ok(id) => id,
        Err(error) => {
            println!("the job did not start: {error}");
            std::process::exit(1);
        }
    };

    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let mut metadata = false;
    let mut downloaded = 0u64;
    let mut peak_peers = 0usize;
    let mut crashed: Option<String> = None;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Some(job) = client.job(&id) else { break };
        metadata |= job.metainfo.is_some();
        downloaded = job.downloaded;
        peak_peers = peak_peers.max(job.peers.len());
        let connected = job.peers.iter().filter(|peer| peer.connected).count();
        println!(
            "t+{:.0}s: {:?}, {} peers ({} connected), {} bytes verified, {} B/s, metadata: {}",
            started.elapsed().as_secs_f64(),
            job.status,
            job.peers.len(),
            connected,
            job.downloaded,
            job.down_speed,
            if job.metainfo.is_some() { "yes" } else { "no" }
        );
        match job.status {
            JobStatus::Completed => break,
            JobStatus::Failed(why) => {
                crashed = Some(why);
                break;
            }
            _ => {}
        }
    }

    println!(
        "summary: metadata {}, {} bytes verified, at most {} peers known",
        if metadata { "arrived" } else { "did not arrive" },
        downloaded,
        peak_peers
    );
    let _ = client.remove(&id, true).await;
    let _ = std::fs::remove_dir_all(&directory);
    if let Some(why) = crashed {
        println!("the job failed: {why}");
        std::process::exit(1);
    }
    if downloaded == 0 {
        println!("no bytes came in; the swarm may be empty or unreachable from this network");
    }
}

/// The bytes of the torrent: a local file, an address, or the current Debian
/// netinst when nothing was given.
async fn read_torrent(source: Option<&str>) -> Result<Vec<u8>, String> {
    let Some(source) = source else {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        let listing = client
            .get(DEBIAN_LIST)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .text()
            .await
            .map_err(|error| error.to_string())?;
        let name = listing
            .split('"')
            .find(|part| part.starts_with("debian-") && part.ends_with("-amd64-netinst.iso.torrent"))
            .ok_or("the Debian listing named no netinst torrent")?;
        let url = format!("{DEBIAN_LIST}{name}");
        println!("using {url}");
        return fetch(&client, &url).await;
    };
    if source.starts_with("http://") || source.starts_with("https://") {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        return fetch(&client, source).await;
    }
    std::fs::read(source).map_err(|error| error.to_string())
}

/// Fetch a torrent, refusing a page that is not one.
async fn fetch(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("the address answered {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err("the response is too large to be a torrent".into());
    }
    Ok(bytes.to_vec())
}

/// A text as it travels in a magnet's query: everything that is not a letter
/// or a digit is written as its escape.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}