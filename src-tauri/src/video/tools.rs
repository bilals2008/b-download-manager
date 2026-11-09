//! The two helpers a video download needs: `yt-dlp` and `ffmpeg`.
//!
//! The app does not ship them. They are fetched on demand from their official
//! releases, checked before they run, and kept in the application's data
//! folder. The installer therefore stays small, and the helpers can be updated
//! without a new build.
//!
//! `yt-dlp` reads a page and names the files behind it. `ffmpeg` puts a picture
//! and its separate sound back together into one file; it is also the only
//! joiner for the mixed containers some sites send.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tauri::Emitter;
use tauri::Manager;

/// Where `yt-dlp` is downloaded from: the publisher's own release, always the
/// latest build (YouTube changes often, and an old helper stops working).
const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const YTDLP_SUMS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";

/// Where `ffmpeg` is downloaded from: the Windows build its author publishes,
/// the smaller essentials build. The zip holds `bin/ffmpeg.exe`.
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

/// The folder a downloaded helper lives in.
pub fn dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("tools"))
}

/// What one helper is and where its program sits.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    pub exe: PathBuf,
}

/// Which helpers are ready right now.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatus {
    pub ytdlp: bool,
    pub ffmpeg: bool,
}

pub fn status(app: &tauri::AppHandle) -> HelperStatus {
    HelperStatus {
        ytdlp: ytdlp(app).is_some(),
        ffmpeg: ffmpeg(app).is_some(),
    }
}

fn programs_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    dir(app).map(|folder| folder.join("programs.json"))
}

/// The helper the person chose and the app verified (`--version`), if still there.
fn chosen(app: &tauri::AppHandle, tool: &str) -> Option<Installed> {
    let file = programs_file(app)?;
    let text = std::fs::read_to_string(&file).ok()?;
    let map: serde_json::Value = serde_json::from_str(&text).ok()?;
    let entry = map.get(tool)?;
    let version = entry.get("version")?.as_str()?.to_string();
    let exe = PathBuf::from(entry.get("exe")?.as_str()?);
    exe.is_file().then_some(Installed { version, exe })
}

/// Keep the person's choice so the app's own download is not used instead.
fn store_chosen(
    app: &tauri::AppHandle,
    tool: &str,
    exe: &Path,
    version: &str,
) -> Result<(), String> {
    let Some(folder) = dir(app) else {
        return Err("the application's data folder is not available".to_string());
    };
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("the helper's folder could not be made: {error}"))?;
    let file = folder.join("programs.json");
    let mut map: serde_json::Value = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::json!({}));
    let entry = serde_json::json!({
        "version": version,
        "exe": exe.to_string_lossy(),
    });
    map.as_object_mut()
        .ok_or("programs.json is not an object")?
        .insert(tool.to_string(), entry);
    let text = serde_json::to_string_pretty(&map).map_err(|error| error.to_string())?;
    let tmp = folder.join("programs.json.tmp");
    std::fs::write(&tmp, text).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|error| error.to_string())?;
    Ok(())
}

/// Forget the person's choices and go back to the app's own copies.
pub fn forget(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(file) = programs_file(app) else {
        return Ok(());
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Ok(());
    };
    let mut map: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::json!({}));
    if let Some(obj) = map.as_object_mut() {
        obj.remove("yt-dlp");
        obj.remove("ffmpeg");
    }
    let text = serde_json::to_string_pretty(&map).map_err(|error| error.to_string())?;
    std::fs::write(&file, text).map_err(|error| error.to_string())?;
    Ok(())
}

/// The `yt-dlp` that may be run right now: the person's choice first
/// (`programs.json`), then the copy the app downloaded, then one beside the
/// executable or on `PATH`.
pub fn ytdlp(app: &tauri::AppHandle) -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("BDM_YTDLP") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(chosen) = chosen(app, "yt-dlp") {
        return Some(chosen.exe);
    }
    if let Some(found) = dir(app).and_then(|folder| in_folder(&folder, "yt-dlp.exe")) {
        return Some(found);
    }
    beside_the_app()
        .into_iter()
        .find_map(|folder| in_folder(&folder, "yt-dlp.exe"))
        .or_else(|| on_path("yt-dlp.exe", "yt-dlp"))
}

/// The `ffmpeg` a join may run. A copy the app knows comes first, then one on
/// `PATH`; the person's own build is fine either way.
pub fn ffmpeg(app: &tauri::AppHandle) -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("BDM_FFMPEG") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(chosen) = chosen(app, "ffmpeg") {
        return Some(chosen.exe);
    }
    if let Some(found) = dir(app).and_then(|folder| in_folder(&folder, "ffmpeg.exe")) {
        return Some(found);
    }
    beside_the_app()
        .into_iter()
        .find_map(|folder| in_folder(&folder, "ffmpeg.exe"))
        .or_else(|| on_path("ffmpeg.exe", "ffmpeg"))
}

fn in_folder(folder: &Path, name: &str) -> Option<PathBuf> {
    let path = folder.join(name);
    path.is_file().then_some(path)
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

fn on_path(windows_name: &str, plain_name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for folder in std::env::split_paths(&path) {
        if let Some(found) =
            in_folder(&folder, windows_name).or_else(|| in_folder(&folder, plain_name))
        {
            return Some(found);
        }
    }
    None
}

/// Run `<exe> --version` and read the version line (`2026.11.01`). Like MDM,
/// the app never runs a tool it has not verified as the real program.
fn version_of(exe: &Path) -> Result<String, String> {
    let output = Command::new(exe)
        .arg("--version")
        .output()
        .map_err(|error| format!("that program could not be run: {error}"))?;
    if !output.status.success() {
        return Err("that program is not the video helper".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .lines()
        .next()
        .ok_or("that program gave no version")?
        .trim();
    if version.is_empty() {
        return Err("that program is not the video helper".to_string());
    }
    Ok(version.to_string())
}

/// Run `ffmpeg -version` and read its first line. Only the real program prints
/// `ffmpeg version ...` there, so a renamed file is refused before it is used.
fn ffmpeg_version(exe: &Path) -> Result<String, String> {
    let output = Command::new(exe)
        .arg("-version")
        .output()
        .map_err(|error| format!("that program could not be run: {error}"))?;
    if !output.status.success() {
        return Err("that program is not ffmpeg".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .next()
        .ok_or("that program gave no version")?
        .trim();
    if !line.to_ascii_lowercase().starts_with("ffmpeg version") {
        return Err("that program is not ffmpeg".to_string());
    }
    Ok(line.to_string())
}

/// Mark a file fetched from the internet as such (Windows shows the "opened
/// from another computer" warning only for downloads the person chose). Like MDM.
#[cfg(windows)]
fn mark_downloaded(path: &Path, from: &str) {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let _ = std::fs::write(
        stream,
        format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={from}\r\n"),
    );
}
#[cfg(not(windows))]
fn mark_downloaded(_path: &Path, _from: &str) {}

/// The proxy the rest of the app already honours, when one is configured.
pub(crate) fn proxy(app: &tauri::AppHandle) -> Option<String> {
    let proxy = app.state::<crate::state::AppState>().limits().proxy();
    proxy
        .map(|proxy| proxy.trim().to_string())
        .filter(|proxy| !proxy.is_empty())
}

/// Download the helper, check it, and keep it. The bytes are only moved into
/// place after the checksum matched, so a half download is never run.
pub async fn install_ytdlp(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let client = client(app)?;
    let expected = published_sum(&client).await?;

    let response = fetch(&client, YTDLP_URL).await.map_err(|error| {
        // A blocked or failing release host must not be a dead end: the panel
        // offers a file the person already downloaded instead.
        format!("the video helper could not be downloaded: {error}")
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
        .map_err(|error| format!("the video helper download stopped: {error}"))?
    {
        hasher.update(&chunk);
        bytes.extend_from_slice(&chunk);
        let _ = app.emit(
            "video-helper://progress",
            serde_json::json!({ "name": "yt-dlp", "downloaded": bytes.len(), "total": total }),
        );
    }

    if hex(&hasher.finalize()) != expected {
        return Err(
            "the video helper did not match its published checksum, so it was not kept".to_string(),
        );
    }
    let (target, version) = save_checked(app, "yt-dlp.exe", &bytes, version_of)?;
    mark_downloaded(&target, YTDLP_URL);
    store_chosen(app, "yt-dlp", &target, &version)?;
    Ok(target)
}

/// Download `ffmpeg`, take its program out of the publisher's zip, check that
/// it runs, and keep it.
pub async fn install_ffmpeg(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let client = client(app)?;
    let response = fetch(&client, FFMPEG_URL)
        .await
        .map_err(|error| format!("ffmpeg could not be downloaded: {error}"))?;
    let total = response.content_length();
    let mut bytes: Vec<u8> = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream
        .next()
        .await
        .transpose()
        .map_err(|error| format!("the ffmpeg download stopped: {error}"))?
    {
        bytes.extend_from_slice(&chunk);
        let _ = app.emit(
            "video-helper://progress",
            serde_json::json!({ "name": "ffmpeg", "downloaded": bytes.len(), "total": total }),
        );
    }

    let program = ffmpeg_from_zip(&bytes)?;
    let (target, version) = save_checked(app, "ffmpeg.exe", &program, ffmpeg_version)?;
    mark_downloaded(&target, FFMPEG_URL);
    store_chosen(app, "ffmpeg", &target, &version)?;
    Ok(target)
}

/// Install whichever of the two helpers is missing. Each one is independent:
/// a page can still be read when only `yt-dlp` arrived.
pub async fn install(app: &tauri::AppHandle) -> Result<(), String> {
    if ytdlp(app).is_none() {
        install_ytdlp(app).await?;
    }
    if ffmpeg(app).is_none() {
        install_ffmpeg(app).await?;
    }
    Ok(())
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
    let (target, version) = save_checked(app, "yt-dlp.exe", &bytes, version_of)?;
    mark_downloaded(&target, YTDLP_URL);
    store_chosen(app, "yt-dlp", &target, &version)?;
    Ok(checked)
}

/// The `ffmpeg.exe` inside the publisher's release zip.
fn ffmpeg_from_zip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| format!("ffmpeg's archive could not be read: {error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("ffmpeg's archive could not be read: {error}"))?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if name == "ffmpeg.exe" || name.ends_with("/bin/ffmpeg.exe") {
            let mut program = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut program).map_err(|error| {
                format!("ffmpeg could not be taken out of its archive: {error}")
            })?;
            if !program.starts_with(b"MZ") {
                return Err("ffmpeg's archive did not hold a Windows program".to_string());
            }
            return Ok(program);
        }
    }
    Err("ffmpeg's archive did not hold ffmpeg.exe".to_string())
}

/// The HTTP client both paths use, with the app's own proxy.
fn client(app: &tauri::AppHandle) -> Result<reqwest::Client, String> {
    // ffmpeg's archive is about 100 MB; a slow link must be allowed to finish.
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(30 * 60));
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
    let list = fetch(client, YTDLP_SUMS_URL)
        .await
        .map_err(|error| format!("the helper's checksum list could not be read: {error}"))?
        .text()
        .await
        .map_err(|error| format!("the helper's checksum list could not be read: {error}"))?;
    expected_sum(&list)
        .ok_or_else(|| "the helper's checksum list did not name yt-dlp.exe".to_string())
}

/// Write the program to a `.part` file, run it once to prove what it is, and
/// only then move it into place. A broken or renamed file never becomes the
/// helper a download would run.
fn save_checked(
    app: &tauri::AppHandle,
    name: &str,
    bytes: &[u8],
    check: fn(&Path) -> Result<String, String>,
) -> Result<(PathBuf, String), String> {
    let folder =
        dir(app).ok_or_else(|| "the application's data folder is not available".to_string())?;
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("the helper's folder could not be made: {error}"))?;
    let target = folder.join(name);
    let part = folder.join(format!("{name}.part"));
    std::fs::write(&part, bytes)
        .map_err(|error| format!("the helper could not be saved: {error}"))?;
    let version = match check(&part) {
        Ok(version) => version,
        Err(error) => {
            let _ = std::fs::remove_file(&part);
            return Err(error);
        }
    };
    // Windows does not rename over an existing file; the old copy goes first.
    let _ = std::fs::remove_file(&target);
    std::fs::rename(&part, &target)
        .map_err(|error| format!("the helper could not be saved: {error}"))?;
    Ok((target, version))
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

#[cfg(test)]
mod zip_tests {
    use super::ffmpeg_from_zip;
    use std::io::{Cursor, Write};

    #[test]
    fn the_program_is_taken_out_of_the_release_zip() {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("ffmpeg-7.1-essentials_build/bin/ffmpeg.exe", options)
                .expect("the entry");
            writer.write_all(b"MZ a program").expect("the bytes");
            writer.finish().expect("the archive");
        }
        assert_eq!(
            ffmpeg_from_zip(buffer.get_ref()).expect("the program"),
            b"MZ a program".to_vec()
        );
    }

    #[test]
    fn an_archive_without_the_program_is_refused() {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            writer
                .start_file("readme.txt", zip::write::SimpleFileOptions::default())
                .expect("the entry");
            writer.write_all(b"nothing here").expect("the bytes");
            writer.finish().expect("the archive");
        }
        assert!(ffmpeg_from_zip(buffer.get_ref()).is_err());
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
