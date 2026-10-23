use super::*;
use crate::segmenter::SEGMENT_TARGET;
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

#[tokio::test]
async fn range_request_lands_at_the_right_offset_in_the_part_file() {
    let data = sample_data();
    let (url, requests) = spawn_range_server(data.clone());
    let dir = temp_dir("range");

    let download = Download::new(1, url, dir.join("out.bin"), 4, None, None).expect("download");
    let mut segments = segmenter::plan(data.len() as u64, 1, &dir, "out");
    segments[0].end = 512;
    *download.segments.lock().expect("lock") = segments;

    fetch_range(
        &download,
        &Limits::default(),
        0,
        0,
        0,
        Some(511),
        download.next_epoch(),
    )
    .await
    .expect("fetch");

    let head = requests.recv().expect("request head").to_ascii_lowercase();
    assert!(head.contains("range: bytes=0-511"), "request was: {head}");
    assert!(
        head.contains("accept-encoding: identity"),
        "request was: {head}"
    );

    let (_, _, downloaded) = download.seg_snapshot(0).expect("segment");
    assert_eq!(downloaded, 512);

    let part = std::fs::read(segmenter::part_path(&dir, "out", 0)).expect("part file");
    assert_eq!(part, data[..512].to_vec());

    let _ = std::fs::remove_dir_all(&dir);
}

/// A server that only serves browsers answers `403` to a download manager
/// that does not name itself like one.
#[tokio::test]
async fn the_request_carries_the_browser_user_agent() {
    let data = sample_data();
    let (url, requests) = spawn_range_server(data.clone());
    let source = crate::transfer::Source::parse(&url, None, None).expect("source");
    source.probe().await.expect("probe");

    let head = requests.recv().expect("request head");
    assert!(
        head.contains(crate::http::USER_AGENT),
        "the request must name itself like a browser, it was: {head}"
    );
}

#[tokio::test]
async fn a_server_that_ignores_the_range_is_rejected() {
    let data = sample_data();
    let url = spawn_ignoring_server(data.clone());
    let dir = temp_dir("ignore");

    let download = Download::new(2, url, dir.join("out.bin"), 4, None, None).expect("download");
    let mut segments = segmenter::plan(data.len() as u64, 1, &dir, "out");
    segments[0].downloaded = 100;
    *download.segments.lock().expect("lock") = segments;

    let error = fetch_range(
        &download,
        &Limits::default(),
        0,
        0,
        100,
        Some(999),
        download.next_epoch(),
    )
    .await
    .expect_err("must fail");
    assert!(
        error.message.contains("ignored the Range request"),
        "error was: {}",
        error.message
    );

    let _ = std::fs::remove_dir_all(&dir);
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
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-/*\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    from
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
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-/*\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    from
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

#[tokio::test]
async fn a_server_that_sends_the_wrong_bytes_goes_on_with_one_connection() {
    // A file of two ranges, so the second one can be checked against the
    // head of the file.
    let data: Vec<u8> = (0..(2 * SEGMENT_TARGET)).map(|i| (i % 251) as u8).collect();
    let url = spawn_lying_server(data.clone());
    let dir = temp_dir("lying");
    let dest = dir.join("out.bin");

    let download = Download::new(12, url, dest.clone(), 4, None, None).expect("download");
    // Keep the test quick: the download fails after two attempts.
    download.max_attempts.store(2, Ordering::SeqCst);
    run(silent(), Limits::default(), download.clone()).await;

    // The range that held the head of the file was given back, and the
    // download went on with a single connection, which the server
    // answers correctly.
    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Measure how many connections are fastest for a server.
///
/// The file to measure comes from the environment, so the measurement is
/// made against a server the reader chooses:
///
/// ```text
/// IDM_BENCH_URL=https://example.com/big.bin cargo test bench_connections -- --ignored --nocapture
/// ```
///
/// The split rule of IDM was not found in the binary (see `ANALYSIS.md`,
/// section 10.4), so the number of connections is a question for a
/// measurement.
#[tokio::test]
#[ignore]
async fn bench_connections() {
    let Ok(url) = std::env::var("IDM_BENCH_URL") else {
        eprintln!("set IDM_BENCH_URL to the file to measure");
        return;
    };
    let dir = temp_dir("bench");

    for connections in 1..=8u32 {
        let dest = dir.join(format!("bench-{connections}.bin"));
        let _ = std::fs::remove_file(&dest);

        let started = Instant::now();
        let download = Download::new(
            connections as u64,
            url.clone(),
            dest.clone(),
            connections,
            None,
            None,
        )
        .expect("download");
        run(silent(), Limits::default(), download.clone()).await;

        let elapsed = started.elapsed();
        let bytes = download.info().downloaded;
        eprintln!(
            "{connections} connections: {elapsed:?} for {bytes} bytes ({:.2} MB/s)",
            bytes as f64 / elapsed.as_secs_f64() / 1_000_000.0
        );
        let _ = std::fs::remove_file(&dest);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_file_of_several_ranges_is_joined_in_the_right_order() {
    // Two ranges, so the parts have to be joined in the right order.
    let data: Vec<u8> = (0..(2 * SEGMENT_TARGET)).map(|i| (i % 251) as u8).collect();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("several-ranges");
    let dest = dir.join("out.bin");

    let download = Download::new(13, url, dest.clone(), 4, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    let written = std::fs::read(&dest).expect("destination");
    assert_eq!(written.len(), data.len(), "the file has the wrong length");
    assert!(
        written == data,
        "the file does not hold the data of the server"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_file_is_downloaded_over_ftp() {
    // A file of two ranges, so the parts have to be joined in order.
    let data: Vec<u8> = (0..(SEGMENT_TARGET + 4 * 1024 * 1024))
        .map(|i| (i % 251) as u8)
        .collect();
    let server = crate::ftp::server::TestServer::start(data.clone());
    let url = format!(
        "ftp://user:secret@127.0.0.1:{}{}",
        server.port,
        crate::ftp::server::PATH
    );
    let dir = temp_dir("ftp");
    let dest = dir.join("out.bin");

    let download = Download::new(14, url, dest.clone(), 4, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    let written = std::fs::read(&dest).expect("destination");
    assert_eq!(written.len(), data.len(), "the file has the wrong length");
    assert!(
        written == data,
        "the file does not hold the data of the server"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_manifest_does_not_hold_the_password() {
    let dir = temp_dir("manifest-password");
    let download = Download::new(
        15,
        "ftp://user:secret@example.com/file.bin".to_string(),
        dir.join("out.bin"),
        4,
        None,
        None,
    )
    .expect("download");

    download.save_manifest();
    let text = std::fs::read_to_string(&download.manifest).expect("manifest");
    assert!(
        !text.contains("secret"),
        "the manifest holds the password: {text}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A finished download whose parts and manifest are gone — which is what
/// every row restored from the index looks like — reports the whole file as
/// downloaded, so the list shows 100% instead of 0%.
#[test]
fn a_finished_download_reports_the_whole_file() {
    let dir = temp_dir("finished-bytes");
    let download = Download::new(
        17,
        "https://example.com/file.bin".to_string(),
        dir.join("out.bin"),
        4,
        None,
        None,
    )
    .expect("download");

    download.total.store(4096, Ordering::SeqCst);
    download.set_status(Status::Completed);

    assert_eq!(download.info().downloaded, 4096);

    let _ = std::fs::remove_dir_all(&dir);
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

#[tokio::test]
async fn a_server_that_is_busy_is_not_mistaken_for_a_small_file() {
    let url = spawn_busy_server();
    let dir = temp_dir("busy");
    let dest = dir.join("out.bin");

    let download = Download::new(16, url, dest.clone(), 4, None, None).expect("download");
    // Keep the test quick.
    download.max_attempts.store(2, Ordering::SeqCst);
    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Failed, "the download must fail");
    let error = info.error.expect("the download must report a failure");
    assert!(error.contains("429"), "error was: {error}");
    assert!(
        !dest.exists(),
        "a download that failed must not leave a file behind"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_cancelled_download_does_not_take_its_task_with_it() {
    let dir = temp_dir("epoch");
    let download = Download::new(
        11,
        "https://example.com/a.bin".into(),
        dir.join("out.bin"),
        4,
        None,
        None,
    )
    .expect("download");

    let first = download.next_epoch();
    assert!(!download.is_cancelled(first));

    download.cancel();
    assert!(download.is_cancelled(first), "the task must stop");

    // A download that is started again gets a new epoch, so it is not
    // cancelled by the cancel of the run before it.
    let second = download.next_epoch();
    assert!(!download.is_cancelled(second));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_paused_download_stops_writing_its_bytes() {
    let data: Vec<u8> = (0..32 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_server(data);
    let dir = temp_dir("pause");
    let download = Download::new(12, url, dir.join("out.bin"), 4, None, None).expect("download");

    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    // Wait until the bytes are flowing.
    let started = Instant::now();
    while download.info().downloaded == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the download never started"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    download.control.pause();
    // The chunk that was already in the socket lands; the next one is not
    // written, because the range sees the pause before it takes the bytes.
    tokio::time::sleep(Duration::from_millis(80)).await;
    let frozen = download.info().downloaded;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        download.info().downloaded,
        frozen,
        "a paused download must stop writing"
    );

    // A cancel has to reach a task that is waiting on the pause control,
    // or the task is left behind for the life of the process.
    download.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("a cancelled download must let its paused task go")
        .expect("the task must not panic");
    assert_eq!(download.info().status, Status::Cancelled);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The progress window lists a row per connection, so the state of each
/// one has to come out of the engine with the download.
#[tokio::test]
async fn a_running_download_reports_the_state_of_its_connections() {
    let data: Vec<u8> = (0..32 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_server(data);
    let dir = temp_dir("connections");
    let download = Download::new(21, url, dir.join("out.bin"), 4, None, None).expect("download");

    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    let started = Instant::now();
    loop {
        let info = download.info();
        if info.resumable.is_some() && info.segments.iter().any(|s| s.downloaded > 0) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the download never reported its connections"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let info = download.info();
    assert_eq!(info.resumable, Some(true), "the server honours ranges");
    assert_eq!(info.speed_limit, 0, "a download starts without a cap");
    assert!(!info.segments.is_empty(), "the plan must be reported");
    assert!(
        info.segments.iter().all(|segment| segment.size.is_some()),
        "the server told the size of the file"
    );
    assert!(
        info.segments.iter().any(|segment| {
            segment.status == SegmentStatus::Receiving || segment.status == SegmentStatus::Done
        }),
        "a connection that is fetching must say so: {:?}",
        info.segments
    );
    let total: u64 = info
        .segments
        .iter()
        .map(|segment| segment.size.unwrap_or(0))
        .sum();
    assert_eq!(total, 32 * 1024, "the ranges cover the whole file");

    download.cancel();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("the download must end")
        .expect("the task must not panic");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A failed download that is resumed is a new attempt, not the failed one:
/// its old message must not be shown beside a file that now finished.
#[tokio::test]
async fn a_restarted_download_does_not_keep_its_old_error() {
    let data = sample_data();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("clear-error");
    let dest = dir.join("out.bin");

    let download = Download::new(26, url, dest.clone(), 2, None, None).expect("download");
    download.set_status(Status::Failed);
    *download.error.lock().expect("lock") = Some("the server was busy".to_string());

    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Completed);
    assert_eq!(info.error, None, "the old failure must be gone");
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_bandwidth_cap_of_one_download_is_reported() {
    let dir = temp_dir("speed-limit-info");
    let download = Download::new(
        22,
        "https://example.com/a.bin".to_string(),
        dir.join("out.bin"),
        4,
        None,
        None,
    )
    .expect("download");

    download.set_speed_limit(64 * 1024);
    assert_eq!(download.info().speed_limit, 64 * 1024);
    assert_eq!(download.speed_limit(), 64 * 1024);

    download.set_speed_limit(0);
    assert_eq!(download.info().speed_limit, 0, "zero means no cap");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_download_read_back_from_the_index_has_no_connection_open() {
    let dir = temp_dir("restored-segments");
    let dest = dir.join("out.bin");
    let known = crate::session::KnownDownload {
        id: 23,
        url: "https://example.com/a.bin".to_string(),
        dest: dest.to_string_lossy().to_string(),
        connections: 4,
        total: 1024,
        downloaded: 512,
        status: Status::Paused,
        error: None,
        referer: None,
        speed_limit: 32 * 1024,
    };

    let download = Download::from_known(&known).expect("download");
    let info = download.info();
    assert_eq!(info.speed_limit, 32 * 1024, "the cap survives a restart");
    assert_eq!(info.status, Status::Paused);
    assert_eq!(
        info.segments.len(),
        0,
        "only a manifest can describe the ranges"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_download_runs_to_the_end_and_leaves_the_file_behind() {
    let data = sample_data();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("end-to-end");
    let dest = dir.join("out.bin");

    let download = Download::new(7, url, dest.clone(), 4, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("destination"), data);
    assert!(
        !download.manifest.exists(),
        "the manifest must be gone after the download"
    );
    let parts: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".part"))
        .collect();
    assert!(parts.is_empty(), "the part files must be gone: {parts:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_download_resumes_from_the_manifest() {
    let data = sample_data();
    let (url, requests) = spawn_range_server(data.clone());
    let dir = temp_dir("resume");
    let dest = dir.join("out.bin");

    let download = Download::new(8, url, dest.clone(), 4, None, None).expect("download");

    // The first run got 400 bytes in and then the app was closed.
    let mut segments = segmenter::plan(data.len() as u64, 1, &dir, &download.stem);
    segments[0].downloaded = 400;
    std::fs::write(&segments[0].part, &data[..400]).expect("part file");
    let manifest = Manifest {
        url: download.url.clone(),
        total: data.len() as u64,
        segments: segments.clone(),
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    // The bytes that were already there must not be asked for again.
    let heads: Vec<String> = std::iter::from_fn(|| requests.try_recv().ok())
        .map(|head| head.to_ascii_lowercase())
        .collect();
    assert!(
        heads.iter().any(|head| head.contains("range: bytes=400-")),
        "the download must continue where it stopped: {heads:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_file_of_an_unknown_size_is_downloaded_as_well() {
    let data = sample_data();
    let url = spawn_unknown_size_server(data.clone());
    let dir = temp_dir("unknown-size");
    let dest = dir.join("out.bin");

    let download = Download::new(9, url, dest.clone(), 4, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Completed);
    // The server honours ranges but does not know the size, so the
    // manifest of the download cannot be matched against it later.
    assert_eq!(info.resumable, Some(false));
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The cap has to be a real limit on the bytes that move, not a number the
/// window displays. A local server hands the file over at once, so the
/// only thing that can make the download take seconds is the limiter.
#[tokio::test]
async fn the_speed_limit_slows_a_real_download() {
    let data: Vec<u8> = (0..64 * 1024u32).map(|i| (i % 251) as u8).collect();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("speed-limit-download");
    let dest = dir.join("out.bin");

    let download = Download::new(24, url, dest.clone(), 4, None, None).expect("download");
    download.set_speed_limit(16 * 1024);

    let started = Instant::now();
    run(silent(), Limits::default(), download.clone()).await;
    let elapsed = started.elapsed();

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);
    assert!(
        elapsed >= Duration::from_secs(2),
        "64 KB at 16 KB/s cannot take less than two seconds, it took {elapsed:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Pause has to stop the bytes, and resume has to pick them up again from
/// the manifest rather than start the file over.
#[tokio::test]
async fn a_paused_download_continues_where_it_stopped() {
    let data: Vec<u8> = (0..64 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_server(data.clone());
    let dir = temp_dir("pause-resume");
    let dest = dir.join("out.bin");

    let download = Download::new(25, url, dest.clone(), 1, None, None).expect("download");
    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    let started = Instant::now();
    while download.info().downloaded == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the download never started"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    download.pause();
    tokio::time::sleep(Duration::from_millis(120)).await;
    let frozen = download.info();
    assert!(
        frozen
            .segments
            .iter()
            .all(|segment| segment.status == SegmentStatus::Paused
                || segment.status == SegmentStatus::Done),
        "every connection must say it is paused: {:?}",
        frozen.segments
    );
    let frozen = frozen.downloaded;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        download.info().downloaded,
        frozen,
        "a paused download must not write another byte"
    );

    download.control.resume();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("a resumed download must finish")
        .expect("the task must not panic");

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_manifest_with_broken_ranges_is_not_resumed_from() {
    let data = sample_data();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("broken-manifest");
    let dest = dir.join("out.bin");

    // Two ranges that overlap: the chunk list of this manifest is
    // damaged, the way a crash in the middle of a write can damage it.
    let download = Download::new(10, url, dest.clone(), 4, None, None).expect("download");
    let segments = vec![
        Segment {
            id: 0,
            start: 0,
            end: 600,
            downloaded: 600,
            part: segmenter::part_path(&dir, &download.stem, 0),
            status: SegmentStatus::Done,
        },
        Segment {
            id: 1,
            start: 512,
            end: 1024,
            downloaded: 0,
            part: segmenter::part_path(&dir, &download.stem, 1),
            status: SegmentStatus::Pending,
        },
    ];
    for segment in &segments {
        std::fs::write(&segment.part, &data[..segment.downloaded as usize]).expect("part");
    }
    let manifest = Manifest {
        url: download.url.clone(),
        total: data.len() as u64,
        segments,
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file whose size the server does not tell ends with its response. A pause
/// drops that response, so the range must not be closed as if the response had
/// ended: doing that produced a short file that was reported as complete.
#[tokio::test]
async fn pausing_an_unknown_size_download_does_not_finish_it_short() {
    let data: Vec<u8> = (0..128 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_unknown_size_server(data.clone());
    let dir = temp_dir("pause-unknown-size");
    let dest = dir.join("out.bin");

    let download = Download::new(41, url, dest.clone(), 1, None, None).expect("download");
    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    let started = Instant::now();
    while download.info().downloaded == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the download never started"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    download.pause();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let frozen = download.info().downloaded;
    assert!(
        frozen > 0 && frozen < data.len() as u64,
        "the pause must land in the middle of the file, it stopped at {frozen}"
    );

    tokio::time::sleep(Duration::from_millis(250)).await;
    let paused = download.info();
    assert_eq!(
        paused.downloaded, frozen,
        "a paused download must not write another byte"
    );
    assert_ne!(
        paused.status,
        Status::Completed,
        "a paused download is not complete"
    );
    assert!(
        !dest.exists(),
        "a paused download must not leave a finished file behind"
    );

    download.control.resume();
    tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .expect("a resumed download must finish")
        .expect("the task must not panic");

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(
        std::fs::read(&dest).expect("destination"),
        data,
        "the resumed file must hold every byte"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A resume asks for a run before the task is spawned. If the user stops the
/// download while that task is still waiting for the previous run to finish,
/// the request is stale and must not start after the stop.
#[tokio::test]
async fn a_run_cancelled_before_it_starts_fetches_nothing() {
    let dir = temp_dir("cancel-before-start");
    let dest = dir.join("out.bin");
    let download = Download::new(
        42,
        "http://127.0.0.1:1/never.bin".to_string(),
        dest.clone(),
        1,
        None,
        None,
    )
    .expect("download");
    download.max_attempts.store(1, Ordering::SeqCst);

    // Hold the driver so the task cannot reach the point where it starts.
    let driver = download.driver.lock().await;
    let request = download.request_run();
    let task = tokio::spawn(run_claimed(
        silent(),
        Limits::default(),
        download.clone(),
        request,
    ));

    download.cancel();
    drop(driver);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("a stale run must return at once")
        .expect("the task must not panic");

    assert!(!download.is_running(), "no task may claim the download");
    assert_eq!(
        download.status(),
        Status::Queued,
        "a run that never started must not change the state"
    );
    assert!(!dest.exists(), "a stale run must not write a file");

    let _ = std::fs::remove_dir_all(&dir);
}

/// A second resume that slips in before the completion window opens must not
/// fetch the finished file again.
#[tokio::test]
async fn a_finished_download_is_not_fetched_again() {
    let dir = temp_dir("completed-run");
    let dest = dir.join("out.bin");
    std::fs::write(&dest, b"done").expect("destination");

    let download = Download::new(
        43,
        "http://127.0.0.1:1/never.bin".to_string(),
        dest.clone(),
        1,
        None,
        None,
    )
    .expect("download");
    download.max_attempts.store(1, Ordering::SeqCst);
    download.total.store(4, Ordering::SeqCst);
    download.set_status(Status::Completed);

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.status(),
        Status::Completed,
        "a finished download must stay finished"
    );
    assert!(!download.is_running());
    assert_eq!(std::fs::read(&dest).expect("destination"), b"done");

    let _ = std::fs::remove_dir_all(&dir);
}

/// A stopped connection is not waiting for anything. The details table says
/// so instead of calling it a range that is about to start.
#[tokio::test]
async fn a_cancelled_download_reports_its_ranges_as_stopped() {
    let data: Vec<u8> = (0..256 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_server(data);
    let dir = temp_dir("stopped-segments");
    let download = Download::new(44, url, dir.join("out.bin"), 1, None, None).expect("download");
    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    let started = Instant::now();
    while download.info().downloaded == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the download never started"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    download.cancel();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("a cancelled download must end")
        .expect("the task must not panic");

    let info = download.info();
    assert_eq!(info.status, Status::Cancelled);
    let unfinished: Vec<_> = info
        .segments
        .iter()
        .filter(|segment| segment.status != SegmentStatus::Done)
        .collect();
    assert!(
        !unfinished.is_empty(),
        "the ranges that were not complete must still be listed: {:?}",
        info.segments
    );
    assert!(
        unfinished
            .iter()
            .all(|segment| segment.status == SegmentStatus::Stopped),
        "an unfinished range of a stopped download is stopped, not waiting: {:?}",
        info.segments
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A connection that answers "come back later" is opened again. The retry is
/// the everyday response to a flaky network, and the file has to survive it.
#[tokio::test]
async fn a_transient_range_failure_is_retried() {
    let data = sample_data();
    let url = spawn_flaky_server(data.clone(), 1);
    let dir = temp_dir("retry");
    let dest = dir.join("out.bin");

    let download = Download::new(46, url, dest.clone(), 1, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);
    assert!(
        download
            .info()
            .segments
            .iter()
            .all(|segment| segment.status == SegmentStatus::Done),
        "the retried range must finish: {:?}",
        download.info().segments
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that starts on one range grows as soon as that range is worth a
/// second connection. A single range has nothing to be compared with, so the
/// balance rule that holds back the others must not hold it at one forever.
#[tokio::test]
async fn a_single_range_gets_a_second_connection() {
    let data: Vec<u8> = (0..2 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_server(data);
    let dir = temp_dir("grow");
    let download = Download::new(47, url, dir.join("out.bin"), 8, None, None).expect("download");
    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    let started = Instant::now();
    while download.info().segments.len() < 3 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the download never added a connection: {:?}",
            download.info().segments
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    download.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("a cancelled download must end")
        .expect("the task must not panic");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The run requests are the guard against a resume that starts after a stop.
#[test]
fn a_cancel_covers_the_run_requests_that_came_before_it() {
    let dir = temp_dir("run-requests");
    let download = Download::new(
        45,
        "https://example.com/a.bin".to_string(),
        dir.join("out.bin"),
        1,
        None,
        None,
    )
    .expect("download");

    let pending = download.request_run();
    assert!(!download.is_superseded(pending), "a fresh request is valid");
    download.cancel();
    assert!(
        download.is_superseded(pending),
        "a cancel covers a request whose task has not started"
    );

    let after = download.request_run();
    assert!(
        !download.is_superseded(after),
        "a resume after the cancel is a new request"
    );

    let newer = download.request_run();
    assert!(
        download.is_superseded(after),
        "a newer request replaces an older one that has not started"
    );
    assert!(!download.is_superseded(newer));

    let _ = std::fs::remove_dir_all(&dir);
}
