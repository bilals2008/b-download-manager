use super::assembly::assemble;
use super::ranges::fetch_range;
use super::*;
use crate::segmenter::MIN_SPLIT;
use std::net::TcpListener;
use std::sync::mpsc;

fn sample_data() -> Vec<u8> {
    (0..1024u32).map(|i| (i % 251) as u8).collect()
}

/// A tiny HTTP server that answers every request with the requested range.
/// Returns the URL and a channel carrying the head of each request.
fn spawn_range_server(data: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();
            let _ = tx.send(head.clone());

            let range = head
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string());

            let (start, end) = match range {
                Some(value) => {
                    let value = value.trim_start_matches("bytes=");
                    let mut parts = value.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .unwrap_or("")
                        .parse()
                        .unwrap_or(data.len().saturating_sub(1));
                    (start, end)
                }
                None => (0, data.len().saturating_sub(1)),
            };
            let end = end.min(data.len().saturating_sub(1));

            let slice = &data[start..=end];
            let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    start,
                    end,
                    data.len()
                );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(slice);
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}/file.bin"), rx)
}

/// A server that ignores Range requests and always answers with 200, slowly
/// enough that the engine gets a chance to think about splitting the file.
fn spawn_slow_ignoring_server(data: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let _ = tx.send(String::from_utf8_lossy(&buf[..read]).to_string());

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                data.len()
            );
            let _ = stream.write_all(response.as_bytes());
            for chunk in data.chunks(256 * 1024) {
                let _ = stream.write_all(chunk);
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}/file.bin"), rx)
}

/// A server that ignores Range requests and always answers with 200.
fn spawn_ignoring_server(data: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap_or(0);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                data.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&data);
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

/// A server that answers the first `failures` data requests with `503`, then
/// serves the range. The probe is always answered, so the test exercises the
/// retry of a range and not the retry of the first question.
fn spawn_flaky_server(data: Vec<u8>, failures: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        let mut left = failures;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();

            let range = head
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string())
                .map(|value| value.trim_start_matches("bytes=").to_string())
                .unwrap_or_else(|| format!("0-{}", data.len().saturating_sub(1)));

            // The probe asks for the first byte only. It must get through, or
            // the test is about the probe rather than about a range retry.
            if range != "0-0" && left > 0 {
                left -= 1;
                let response =
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(response);
                let _ = stream.flush();
                continue;
            }

            let mut parts = range.split('-');
            let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let end: usize = parts
                .next()
                .unwrap_or("")
                .parse()
                .unwrap_or(data.len().saturating_sub(1))
                .min(data.len().saturating_sub(1));
            let slice = &data[start..=end];

            let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    start,
                    end,
                    data.len()
                );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(slice);
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

/// A server whose answer is built from the request head and the number of
/// requests it has seen. `None` closes the connection without an answer.
fn spawn_scripted_server(
    answer: impl Fn(&str, usize) -> Option<Vec<u8>> + Send + 'static,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for (seen, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();
            let _ = tx.send(head.clone());
            if let Some(response) = answer(&head, seen) {
                let _ = stream.write_all(&response);
                let _ = stream.flush();
            }
        }
    });

    (format!("http://127.0.0.1:{port}/file.bin"), rx)
}

/// The byte range a request asks for, with the end defaulting to the last
/// byte of the file.
fn requested_range(head: &str, len: usize) -> (usize, usize) {
    let value = head
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("range:"))
        .and_then(|line| line.split(':').nth(1))
        .map(|value| value.trim().trim_start_matches("bytes=").to_string())
        .unwrap_or_else(|| format!("0-{}", len.saturating_sub(1)));

    let mut parts = value.split('-');
    let start = parts
        .next()
        .and_then(|start| start.parse().ok())
        .unwrap_or(0);
    let end = parts
        .next()
        .and_then(|end| end.parse().ok())
        .unwrap_or(len.saturating_sub(1))
        .min(len.saturating_sub(1));
    (start, end)
}

/// A `206` answer for the bytes `start..=end` of `data`, with extra headers.
fn partial_response(data: &[u8], start: usize, end: usize, extra: &str) -> Vec<u8> {
    let end = end.min(data.len().saturating_sub(1));
    let slice: &[u8] = if start <= end {
        &data[start..=end]
    } else {
        &[]
    };
    let mut response = format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n{}\r\n",
        slice.len(),
        data.len(),
        extra
    )
    .into_bytes();
    response.extend_from_slice(slice);
    response
}

/// The heads of the requests the server has answered so far, in lower case.
fn requests_so_far(requests: &mpsc::Receiver<String>) -> Vec<String> {
    std::iter::from_fn(|| requests.try_recv().ok())
        .map(|head| head.to_ascii_lowercase())
        .collect()
}

/// A server that gives every connection its own share of the bandwidth: it
/// answers a range in chunks and waits between them, on a thread per
/// connection. Two connections therefore cost twice the wall clock of one
/// only because this server is built that way, which is what makes it a fair
/// way to measure whether the engine uses the connections it opens.
fn spawn_paced_server(data: Vec<u8>, chunk: usize, delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let data = Arc::new(data);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let data = data.clone();
            std::thread::spawn(move || {
                let _ = stream.set_nodelay(true);
                let mut buf = [0u8; 4096];
                let Ok(read) = stream.read(&mut buf) else {
                    return;
                };
                let head = String::from_utf8_lossy(&buf[..read]).to_string();
                let (start, end) = requested_range(&head, data.len());
                let slice = &data[start..=end];
                let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    data.len()
                );
                if stream.write_all(response.as_bytes()).is_err() {
                    return;
                }
                for piece in slice.chunks(chunk) {
                    if stream.write_all(piece).is_err() {
                        return;
                    }
                    std::thread::sleep(delay);
                }
            });
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

/// A server that drips the range it was asked for, a chunk at a time, so a
/// test has a transfer that is still running when it pauses it.
fn spawn_slow_server(data: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();

            let range = head
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string())
                .map(|value| {
                    let value = value.trim_start_matches("bytes=");
                    let mut parts = value.split('-');
                    let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                    let end: usize = parts
                        .next()
                        .unwrap_or("")
                        .parse()
                        .unwrap_or(data.len().saturating_sub(1));
                    (start, end)
                })
                .unwrap_or((0, data.len().saturating_sub(1)));
            let (start, end) = (range.0.min(data.len() - 1), range.1.min(data.len() - 1));

            let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    end - start + 1,
                    start,
                    end,
                    data.len()
                );
            let _ = stream.write_all(response.as_bytes());

            // The probe asks for one byte; the rest is streamed slowly.
            let step = if start == 0 && end == 0 { 1 } else { 1024 };
            for chunk in data[start..=end].chunks(step) {
                if stream.write_all(chunk).is_err() {
                    break;
                }
                let _ = stream.flush();
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "my-idm-clone-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A reporter that throws the updates away.
fn silent() -> Arc<dyn Reporter> {
    Arc::new(SilentReporter)
}

/// A server that answers a range request but does not know the size of
/// the file: `Content-Range: bytes 0-0/*`.
fn spawn_unknown_size_server(data: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();

            let from = head
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .and_then(|line| line.split(':').nth(1))
                .map(|value| {
                    value
                        .trim()
                        .trim_start_matches("bytes=")
                        .split('-')
                        .next()
                        .unwrap_or("0")
                        .parse::<usize>()
                        .unwrap_or(0)
                })
                .unwrap_or(0);

            let slice = &data[from.min(data.len())..];
            let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{end}/*\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    from,
                    end = data.len().saturating_sub(1)
                );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(slice);
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

/// A server that honours ranges but does not know the size of the file, and
/// drips the bytes so a test can pause the transfer in the middle.
fn spawn_slow_unknown_size_server(data: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                continue;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();

            let from = head
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .and_then(|line| line.split(':').nth(1))
                .map(|value| {
                    value
                        .trim()
                        .trim_start_matches("bytes=")
                        .split('-')
                        .next()
                        .unwrap_or("0")
                        .parse::<usize>()
                        .unwrap_or(0)
                })
                .unwrap_or(0)
                .min(data.len().saturating_sub(1));

            let slice = &data[from..];
            let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{end}/*\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    from,
                    end = data.len().saturating_sub(1)
                );
            if stream.write_all(response.as_bytes()).is_err() {
                continue;
            }
            for chunk in slice.chunks(1024) {
                if stream.write_all(chunk).is_err() {
                    break;
                }
                let _ = stream.flush();
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

/// A server that answers a range request with the head of the file when
/// the range does not start at the first byte: a server that ignores the
/// range it was asked for.
fn spawn_lying_server(data: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            let head = String::from_utf8_lossy(&buf[..read]).to_string();

            let range = head
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .and_then(|line| line.split(':').nth(1))
                .map(|value| value.trim().trim_start_matches("bytes=").to_string())
                .unwrap_or_else(|| format!("0-{}", data.len() - 1));

            let mut parts = range.split('-');
            let from: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let to: usize = parts
                .next()
                .unwrap_or("")
                .parse()
                .unwrap_or(data.len() - 1)
                .min(data.len() - 1);

            // The header says the range that was asked for. The body is
            // the data of that range, unless the range does not start at
            // the first byte: then the head of the file is sent again.
            let slice = if from == 0 {
                &data[from..=to]
            } else {
                &data[..=(to - from).min(data.len() - 1)]
            };

            let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    from,
                    to,
                    data.len()
                );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(slice);
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

/// A server that is busy: it answers `429` with a body of one byte.
fn spawn_busy_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap_or(0);
            let response = b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx";
            let _ = stream.write_all(response);
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}/file.bin")
}

mod assembly;
mod control;
mod probe;
mod ranges;
mod resume;
