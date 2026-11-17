//! Fetching ranges: offsets, retries, redirects and the pace of a download.

use super::*;

#[tokio::test]
async fn a_part_file_that_cannot_be_written_fails_at_once() {
    let data = sample_data();
    let served = data.clone();
    let (url, requests) = spawn_scripted_server(move |head, _| {
        let (start, end) = requested_range(head, served.len());
        Some(partial_response(&served, start, end, ""))
    });
    let dir = temp_dir("disk-error");
    let dest = dir.join("out.bin");
    let download = Download::new(56, url, dest.clone(), 1, None, None).expect("download");
    download.max_attempts.store(4, Ordering::SeqCst);

    // A directory where the part file should be: the write cannot work, and
    // no number of retries can make it work.
    std::fs::create_dir_all(segmenter::part_path(&dir, &download.stem, 0)).expect("part dir");

    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Failed);
    assert!(
        info.error.is_some(),
        "the reason the part could not be opened"
    );
    assert_eq!(
        requests_so_far(&requests).len(),
        2,
        "a disk that cannot be written is not asked again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_redirected_download_finishes() {
    let data = sample_data();
    let served = data.clone();
    let (url, requests) = spawn_scripted_server(move |head, _| {
        let request_line = head.lines().next().unwrap_or("");
        if request_line.contains(" /file.bin ") {
            return Some(
                b"HTTP/1.1 302 Found\r\nLocation: /real.bin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            );
        }
        let (start, end) = requested_range(head, served.len());
        Some(partial_response(&served, start, end, ""))
    });
    let dir = temp_dir("redirect");
    let dest = dir.join("out.bin");
    let download = Download::new(58, url, dest.clone(), 1, None, None).expect("download");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);
    assert!(
        requests_so_far(&requests)
            .iter()
            .any(|head| head.contains(" /real.bin ")),
        "the redirect must be followed"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Compare one connection with four against a server that gives every
/// connection the same paced bandwidth.
///
/// The server is artificial on purpose: it isolates the engine's use of
/// connections from the network between this machine and a real server. Four
/// connections can only win here because the server hands each of them its
/// own budget; a real site may cap it, which is why the engine does not
/// assume that more is faster.
#[tokio::test]
#[ignore = "a measurement, not a test: cargo test --release bench_local -- --ignored --nocapture"]
async fn bench_local_segmented() {
    const CHUNK: usize = 256 * 1024;
    let delay = Duration::from_millis(15);
    // Four ranges of sixteen megabytes: the plan is four ranges when four
    // connections are allowed and one when only one is.
    let data: Vec<u8> = (0..4 * SEGMENT_TARGET).map(|i| (i % 251) as u8).collect();
    let url = spawn_paced_server(data.clone(), CHUNK, delay);
    let dir = temp_dir("bench-local");

    for connections in [1u32, 4] {
        let dest = dir.join(format!("bench-{connections}.bin"));
        let download = Download::new(
            60 + connections as u64,
            url.clone(),
            dest.clone(),
            connections,
            None,
            None,
        )
        .expect("download");

        let started = Instant::now();
        run(silent(), Limits::default(), download.clone()).await;
        let elapsed = started.elapsed();

        assert_eq!(
            download.info().status,
            Status::Completed,
            "error: {:?}",
            download.info().error
        );
        assert_eq!(
            std::fs::read(&dest).expect("destination"),
            data,
            "the file must hold every byte"
        );
        eprintln!(
            "{connections} connection(s): {elapsed:?} for {} bytes ({:.1} MB/s)",
            data.len(),
            data.len() as f64 / elapsed.as_secs_f64() / 1_000_000.0
        );
        let _ = std::fs::remove_file(&dest);
    }

    let _ = std::fs::remove_dir_all(&dir);
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
/// BDM_BENCH_URL=https://example.com/big.bin cargo test bench_connections -- --ignored --nocapture
/// ```
///
/// The split rule of the reference download manager was not found in the
/// binary (the open question is priority 1 in `planning/02-rea-plan.md`), so
/// the number of connections is a question for a measurement.
#[tokio::test]
#[ignore]
async fn bench_connections() {
    let Ok(url) = std::env::var("BDM_BENCH_URL") else {
        eprintln!("set BDM_BENCH_URL to the file to measure");
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
