//! The one helper a YouTube download needs: `yt-dlp`.
//!
//! The app does not ship it. It is fetched on demand from its official
//! release, checked against the publisher's own SHA-256 list, and kept in the
//! application's data folder. The installer therefore stays small, and the
//! helper can be updated without a new build.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tauri::Emitter;
use tauri::Manager;

/// Where the helper is downloaded from: the publisher's own release, always
/// the latest build (YouTube changes often, and an old helper stops working).
const HELPER_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const CHECKSUMS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";

/// The folder a downloaded helper lives in.
pub fn dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("tools"))
}

/// The helper that may be run right now: an explicit override, the copy the
/// app downloaded, then one beside the executable or on `PATH` (a power user
/// may already have it).
pub fn installed(app: &tauri::AppHandle) -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("BDM_YTDLP") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(found) = dir(app).and_then(|folder| in_folder(&folder)) {
        return Some(found);
    }
    beside_the_app()
        .into_iter()
        .find_map(|folder| in_folder(&folder))
        .or_else(on_path)
}

fn in_folder(folder: &Path) -> Option<PathBuf> {
    for name in ["yt-dlp.exe", "yt-dlp"] {
        let path = folder.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn beside_the_app() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            folders.push(parent.to_path_buf());
        }
    }
    folders
}

fn on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for folder in std::env::split_paths(&path) {
        if let Some(found) = in_folder(&folder) {
            return Some(found);
        }
        #[cfg(windows)]
        {
            let candidate = folder.join("yt-dlp.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The proxy the rest of the app already honours, when one is configured.
pub(crate) fn proxy(app: &tauri::AppHandle) -> Option<String> {
    let proxy = app.state::<crate::state::AppState>().limits().proxy();
    proxy
        .map(|proxy| proxy.trim().to_string())
        .filter(|proxy| !proxy.is_empty())
}

/// Download the helper, check it, and keep it. The bytes are only moved into
/// place after the checksum matched, so a half download is never run.
pub async fn install(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let client = client(app)?;
    let expected = published_sum(&client).await?;

    let response = fetch(&client, HELPER_URL).await.map_err(|error| {
        // A blocked or failing release host must not be a dead end: the panel
        // offers a file the person already downloaded instead.
        format!("the helper could not be downloaded: {error}")
    })?;
    let total = response.content_length();

    let mut hasher = Sha256::new();
    let mut bytes: Vec<u8> = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream
        .next()
        .await
        .transpose()
        .map_err(|error| format!("the helper download stopped: {error}"))?
    {
        hasher.update(&chunk);
        bytes.extend_from_slice(&chunk);
        let _ = app.emit(
            "video-helper://progress",
            serde_json::json!({ "downloaded": bytes.len(), "total": total }),
        );
    }

    if hex(&hasher.finalize()) != expected {
        return Err(
            "the helper did not match its published checksum, so it was not kept".to_string(),
        );
    }
    save(app, &bytes)
}

/// Keep a helper the person already downloaded, with the same check as a
/// download. The answer is whether the publisher's list could be reached: a
/// copy that could not be checked is still kept, because the choice of file
/// was the person's own.
pub async fn import(app: &tauri::AppHandle, source: &Path) -> Result<bool, String> {
    let bytes =
        std::fs::read(source).map_err(|error| format!("that file could not be read: {error}"))?;
    if bytes.len() < 1_000_000 {
        return Err("that file is too small to be the video helper".to_string());
    }
    if !bytes.starts_with(b"MZ") {
        return Err("that file is not a Windows program".to_string());
    }
    let actual = hex(&Sha256::digest(&bytes));
    let checked = match published_sum(&client(app)?).await {
        Ok(published) if published == actual => true,
        Ok(_) => {
            return Err(
                "that file did not match the published checksum, so it was not kept".to_string(),
            )
        }
        Err(_) => false,
    };
    save(app, &bytes)?;
    Ok(checked)
}

/// The HTTP client both paths use, with the app's own proxy.
fn client(app: &tauri::AppHandle) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(300));
    if let Some(proxy) = proxy(app) {
        let proxy = reqwest::Proxy::all(&proxy)
            .map_err(|error| format!("the proxy address is not usable: {error}"))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|error| format!("the download could not be set up: {error}"))
}

/// The SHA-256 the publisher lists for `yt-dlp.exe` right now.
async fn published_sum(client: &reqwest::Client) -> Result<String, String> {
    let list = fetch(client, CHECKSUMS_URL)
        .await
        .map_err(|error| format!("the helper's checksum list could not be read: {error}"))?
        .text()
        .await
        .map_err(|error| format!("the helper's checksum list could not be read: {error}"))?;
    expected_sum(&list)
        .ok_or_else(|| "the helper's checksum list did not name yt-dlp.exe".to_string())
}

/// Move the bytes into place. A `.part` file means a broken write is never the
/// file that gets run.
fn save(app: &tauri::AppHandle, bytes: &[u8]) -> Result<PathBuf, String> {
    let folder = dir(app)
        .ok_or_else(|| "the application's data folder is not available".to_string())?;
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("the helper's folder could not be made: {error}"))?;
    let target = folder.join("yt-dlp.exe");
    let part = folder.join("yt-dlp.exe.part");
    std::fs::write(&part, bytes)
        .map_err(|error| format!("the helper could not be saved: {error}"))?;
    std::fs::rename(&part, &target)
        .map_err(|error| format!("the helper could not be saved: {error}"))?;
    Ok(target)
}

/// One release address, with a couple of retries. The asset CDN answers a
/// `504` now and then, and a second attempt usually gets the file.
async fn fetch(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, String> {
    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let error = match client.get(url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => return Ok(response),
                Err(error) => error.to_string(),
            },
            Err(error) => error.to_string(),
        };
        last = error;
    }
    Err(last)
}

/// The SHA-256 the publisher listed for `yt-dlp.exe`.
fn expected_sum(list: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let sum = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == "yt-dlp.exe").then(|| sum.to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod checksum_tests {
    use super::expected_sum;

    #[test]
    fn the_helpers_line_is_read_from_the_published_list() {
        let list = "aaaaaaaa\tamd64/yt-dlp\nbbbbbbbb  yt-dlp.exe\ncccc  _yt-dlp\n";
        assert_eq!(expected_sum(list), Some("bbbbbbbb".to_string()));
        assert_eq!(expected_sum("nothing here"), None);
    }
}

/// One lock for every test that manipulates a process-wide override. Tests
/// share a process, so an environment variable set by one must not reach
/// another module's test.
#[cfg(test)]
pub fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One EBML element with a one- or two-byte size, for building fixtures.
#[cfg(test)]
pub fn ebml(id: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    let size = body.len();
    if size < 0x7F {
        out.push(0x80 | size as u8);
    } else {
        out.push(0x40 | (size >> 8) as u8);
        out.push(size as u8);
    }
    out.extend_from_slice(body);
    out
}

/// A whole one-second WebM: one picture track holding one frame.
///
/// It is assembled from the bytes rather than encoded, so it is the same file
/// on every machine and needs no video tool to exist.
#[cfg(test)]
pub fn tiny_webm() -> Vec<u8> {
    let header = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"webm"));
    let mut info = ebml(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40]);
    info.extend(ebml(&[0x44, 0x89], &1000.0f64.to_be_bytes()));
    let mut entry = ebml(&[0xD7], &[1]);
    entry.extend(ebml(&[0x83], &[1]));
    entry.extend(ebml(&[0x86], b"V_VP9"));
    let mut cluster = ebml(&[0xE7], &[0]);
    cluster.extend(ebml(&[0xA3], &[0x81, 0, 0, 0x80, 1, 2, 3]));

    let mut out = header;
    out.extend([0x18, 0x53, 0x80, 0x67]);
    out.extend([0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    out.extend(ebml(&[0x15, 0x49, 0xA9, 0x66], &info));
    out.extend(ebml(&[0x16, 0x54, 0xAE, 0x6B], &ebml(&[0xAE], &entry)));
    out.extend(ebml(&[0x1F, 0x43, 0xB6, 0x75], &cluster));
    out
}
