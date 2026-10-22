
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
                let Ok(read) = stream.read(&mut buf) else { break };
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

        let download = Download::new(1, url, dir.join("out.bin"), 4, None).expect("download");
        let mut segments = segmenter::plan(data.len() as u64, 1, &dir, "out");
        segments[0].end = 512;
        *download.segments.lock().expect("lock") = segments;

        fetch_range(&download, &Limits::default(), 0, 0, 0, Some(511))
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
        let source = crate::transfer::Source::parse(&url, None).expect("source");
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

        let download = Download::new(2, url, dir.join("out.bin"), 4, None).expect("download");
        let mut segments = segmenter::plan(data.len() as u64, 1, &dir, "out");
        segments[0].downloaded = 100;
        *download.segments.lock().expect("lock") = segments;

        let error = fetch_range(&download, &Limits::default(), 0, 0, 100, Some(999))
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
                let Ok(read) = stream.read(&mut buf) else { break };
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
                let Ok(read) = stream.read(&mut buf) else { break };
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
        let data: Vec<u8> = (0..(2 * SEGMENT_TARGET))
            .map(|i| (i % 251) as u8)
            .collect();
        let url = spawn_lying_server(data.clone());
        let dir = temp_dir("lying");
        let dest = dir.join("out.bin");

        let download = Download::new(12, url, dest.clone(), 4, None).expect("download");
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
            let download =
                Download::new(connections as u64, url.clone(), dest.clone(), connections, None)
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
        let data: Vec<u8> = (0..(2 * SEGMENT_TARGET))
            .map(|i| (i % 251) as u8)
            .collect();
        let (url, _requests) = spawn_range_server(data.clone());
        let dir = temp_dir("several-ranges");
        let dest = dir.join("out.bin");

        let download = Download::new(13, url, dest.clone(), 4, None).expect("download");
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

        let download = Download::new(14, url, dest.clone(), 4, None).expect("download");
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

        let download = Download::new(16, url, dest.clone(), 4, None).expect("download");
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
        let download = Download::new(11, "https://example.com/a.bin".into(), dir.join("out.bin"), 4, None)
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
    async fn a_download_runs_to_the_end_and_leaves_the_file_behind() {
        let data = sample_data();
        let (url, _requests) = spawn_range_server(data.clone());
        let dir = temp_dir("end-to-end");
        let dest = dir.join("out.bin");

        let download = Download::new(7, url, dest.clone(), 4, None).expect("download");
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

        let download = Download::new(8, url, dest.clone(), 4, None).expect("download");

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

        let download = Download::new(9, url, dest.clone(), 4, None).expect("download");
        run(silent(), Limits::default(), download.clone()).await;

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
        let download = Download::new(10, url, dest.clone(), 4, None).expect("download");
        let segments = vec![
            Segment {
                id: 0,
                start: 0,
                end: 600,
                downloaded: 600,
                part: segmenter::part_path(&dir, &download.stem, 0),
            },
            Segment {
                id: 1,
                start: 512,
                end: 1024,
                downloaded: 0,
                part: segmenter::part_path(&dir, &download.stem, 1),
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