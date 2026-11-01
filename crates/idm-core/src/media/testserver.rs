//! A small HTTP server, for the media tests.
//!
//! It serves a fixture directory or a set of in-memory files, answers `Range`
//! requests with `206` and a correct `Content-Range`, and can be told to fail
//! one path with a status, to drop a connection part way through a body, or to
//! count the requests a path received. The media tests need all of those: a
//! resume must not download a segment twice, and a 403 must not become a file.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A server that serves one fixture set.
pub struct TestServer {
    port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
}

struct State {
    files: HashMap<String, Vec<u8>>,
    /// How many requests each path received.
    counts: HashMap<String, u32>,
    /// A status that the next requests for a path answer with, and how many.
    failures: HashMap<String, (u16, u32)>,
    /// Drop the body of the next request for a path after this many bytes.
    drops: HashMap<String, u32>,
    /// The first value each path received for a header, keyed by lower-case
    /// name. The header tests read this to prove what travelled.
    headers: HashMap<(String, String), String>,
    /// A header a path insists on, name and value. A request without it is
    /// refused with 403, the way a signed-in site refuses an anonymous one.
    required: HashMap<String, (String, String)>,
}

impl TestServer {
    /// Serve every file under `root`, at its path relative to `root`.
    pub fn start(root: impl Into<PathBuf>) -> Self {
        let mut files = HashMap::new();
        let root = root.into();
        load_dir(&root, &root, &mut files);
        Self::start_with_files(files)
    }

    /// Serve the given paths with the given bytes. A path may or may not start
    /// with `/`; both name the same address.
    pub fn start_with_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let files = files
            .into_iter()
            .map(|(path, bytes)| (normalise(&path), bytes))
            .collect();
        let state = Arc::new(Mutex::new(State {
            files,
            counts: HashMap::new(),
            failures: HashMap::new(),
            drops: HashMap::new(),
            headers: HashMap::new(),
            required: HashMap::new(),
        }));
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the fixture server");
        let port = listener.local_addr().expect("the port").port();
        let stop = Arc::new(AtomicBool::new(false));

        let accepted = state.clone();
        let running = stop.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if running.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { break };
                let state = accepted.clone();
                std::thread::spawn(move || {
                    let _ = serve(stream, state);
                });
            }
        });

        Self { port, state, stop }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The loopback URL of a path.
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, normalise(path))
    }

    /// The first value a path received for a header, lower-case name.
    pub fn header(&self, path: &str, name: &str) -> Option<String> {
        self.state
            .lock()
            .expect("the fixture state")
            .headers
            .get(&(normalise(path), name.to_ascii_lowercase()))
            .cloned()
    }

    /// How many requests a path has received.
    pub fn count(&self, path: &str) -> u32 {
        self.state
            .lock()
            .expect("the fixture state")
            .counts
            .get(&normalise(path))
            .copied()
            .unwrap_or(0)
    }

    /// Answer every later request for a path with this status.
    pub fn fail(&self, path: &str, status: u16) {
        self.state
            .lock()
            .expect("the fixture state")
            .failures
            .insert(normalise(path), (status, u32::MAX));
    }

    /// Answer the next request for a path with this status, then serve it.
    pub fn fail_next(&self, path: &str, status: u16) {
        self.state
            .lock()
            .expect("the fixture state")
            .failures
            .insert(normalise(path), (status, 1));
    }

    /// Drop the body of the next request for a path after `bytes`.
    pub fn drop_next(&self, path: &str, bytes: u32) {
        self.state
            .lock()
            .expect("the fixture state")
            .drops
            .insert(normalise(path), bytes);
    }

    /// Make a path require a header and value, or answer 403.
    pub fn require(&self, path: &str, name: &str, value: &str) {
        self.state
            .lock()
            .expect("the fixture state")
            .required
            .insert(
                normalise(path),
                (name.to_ascii_lowercase(), value.to_string()),
            );
    }

    pub fn clear_failures(&self) {
        let mut state = self.state.lock().expect("the fixture state");
        state.failures.clear();
        state.drops.clear();
    }

    /// Stop the listener. A server that is dropped stops itself.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking accept.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn load_dir(root: &Path, folder: &Path, files: &mut HashMap<String, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_dir(root, &path, files);
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        files.insert(normalise(&name), bytes);
    }
}

fn normalise(path: &str) -> String {
    let trimmed = path.trim().trim_start_matches('/');
    format!("/{trimmed}")
}

fn serve(mut stream: TcpStream, state: Arc<Mutex<State>>) -> std::io::Result<()> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > 16 * 1024 {
            return write_status(&mut stream, 431, "Request Header Fields Too Large", b"");
        }
    }

    let text = String::from_utf8_lossy(&buffer);
    let mut lines = text.split("\r\n");
    let Some(request_line) = lines.next() else {
        return write_status(&mut stream, 400, "Bad Request", b"");
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let path = normalise(target.split('?').next().unwrap_or(""));

    let method = match method {
        "GET" => true,
        "HEAD" => false,
        _ => return write_status(&mut stream, 405, "Method Not Allowed", b""),
    };

    let mut range = None;
    let mut received: Vec<(String, String)> = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        if name == "range" {
            range = parse_range(value.trim());
        }
        received.push((name, value.trim().to_string()));
    }

    let (bytes, failure, drop_after, required) = {
        let mut state = state.lock().expect("the fixture state");
        *state.counts.entry(path.clone()).or_insert(0) += 1;
        for (name, value) in &received {
            state
                .headers
                .entry((path.clone(), name.clone()))
                .or_insert(value.clone());
        }
        let required = state.required.get(&path).cloned();
        let failure = match state.failures.get_mut(&path) {
            Some((status, left)) if *left > 0 => {
                let status = *status;
                if *left != u32::MAX {
                    *left -= 1;
                }
                Some(status)
            }
            _ => None,
        };
        let drop_after = state.drops.remove(&path);
        (
            state.files.get(&path).cloned(),
            failure,
            drop_after,
            required,
        )
    };

    if let Some((name, value)) = required {
        let sent = received
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, value)| value.as_str());
        if sent != Some(value.as_str()) {
            return write_status(&mut stream, 403, "Forbidden", b"a cookie is required");
        }
    }

    if let Some(status) = failure {
        let reason = match status {
            403 => "Forbidden",
            404 => "Not Found",
            410 => "Gone",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "Error",
        };
        return write_status(&mut stream, status, reason, b"the fixture said no");
    }

    let Some(bytes) = bytes else {
        return write_status(
            &mut stream,
            404,
            "Not Found",
            b"the fixture has no such path",
        );
    };

    let total = bytes.len() as u64;
    let (status, body, content_range) = match range {
        Some((from, _)) if from >= total => (416, Vec::new(), Some(format!("bytes */{total}"))),
        Some((from, to)) => {
            let end = to.min(total.saturating_sub(1));
            let body = bytes[from as usize..=end as usize].to_vec();
            (206, body, Some(format!("bytes {from}-{end}/{total}")))
        }
        None => (200, bytes, None),
    };

    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    head.push_str("Connection: close\r\n");
    head.push_str("Accept-Ranges: bytes\r\n");
    head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    if let Some(content_range) = content_range {
        head.push_str(&format!("Content-Range: {content_range}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;

    if method {
        match drop_after {
            // Write only part of the body and close: the client sees a
            // connection that ended early, which is what a resume test needs.
            Some(limit) => {
                let limit = (limit as usize).min(body.len());
                stream.write_all(&body[..limit])?;
            }
            None => stream.write_all(&body)?,
        }
    }
    stream.flush()
}

fn write_status(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        416 => "Range Not Satisfiable",
        _ => "Status",
    }
}

/// `bytes=<from>-<to>` or `bytes=<from>-`.
fn parse_range(value: &str) -> Option<(u64, u64)> {
    let rest = value.strip_prefix("bytes=")?;
    let (from, to) = rest.split_once('-')?;
    let from: u64 = from.trim().parse().ok()?;
    let to = if to.trim().is_empty() {
        u64::MAX
    } else {
        to.trim().parse().ok()?
    };
    if to < from {
        return None;
    }
    Some((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(server: &TestServer, path: &str, range: Option<&str>) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port())).expect("connect");
        let mut request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n");
        if let Some(range) = range {
            request.push_str(&format!("Range: {range}\r\n"));
        }
        request.push_str("Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).expect("write");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("read");
        answer
    }

    #[test]
    fn a_file_is_served_whole_and_in_a_range() {
        let server =
            TestServer::start_with_files(vec![("/file.bin".to_string(), (0u8..100).collect())]);

        let whole = get(&server, "/file.bin", None);
        assert!(whole.starts_with("HTTP/1.1 200 OK"));
        assert!(whole.contains("Content-Length: 100"));

        let part = get(&server, "/file.bin", Some("bytes=10-19"));
        assert!(part.starts_with("HTTP/1.1 206 Partial Content"));
        assert!(part.contains("Content-Range: bytes 10-19/100"));
        let expected: String = (10u8..20).map(char::from).collect();
        assert!(part.ends_with(&expected));
        assert_eq!(server.count("/file.bin"), 2);
    }

    #[test]
    fn a_missing_path_and_a_told_failure_are_not_files() {
        let server = TestServer::start_with_files(vec![("/file.bin".to_string(), b"ok".to_vec())]);
        assert!(get(&server, "/nope", None).starts_with("HTTP/1.1 404"));

        server.fail_next("/file.bin", 403);
        assert!(get(&server, "/file.bin", None).starts_with("HTTP/1.1 403"));
        assert!(get(&server, "/file.bin", None).starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn a_range_header_is_read() {
        assert_eq!(parse_range("bytes=0-9"), Some((0, 9)));
        assert_eq!(parse_range("bytes=100-"), Some((100, u64::MAX)));
        assert_eq!(parse_range("bytes=9-0"), None);
        assert_eq!(parse_range("items=0-9"), None);
    }
}
