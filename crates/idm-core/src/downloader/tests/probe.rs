//! Probing the server: validators, content ranges and the answers it gives.

use super::*;

#[tokio::test]
async fn every_range_carries_the_validator_of_the_probe() {
    let data = sample_data();
    let served = data.clone();
    let (url, requests) = spawn_scripted_server(move |head, _| {
        let (start, end) = requested_range(head, served.len());
        Some(partial_response(&served, start, end, "ETag: \"v1\"\r\n"))
    });
    let dir = temp_dir("if-range");
    let dest = dir.join("out.bin");

    let download = Download::new(50, url, dest.clone(), 1, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let heads = requests_so_far(&requests);
    let data_requests: Vec<&String> = heads
        .iter()
        .filter(|head| !head.contains("range: bytes=0-0\r\n"))
        .collect();
    assert!(!data_requests.is_empty(), "the range was never asked for");
    assert!(
        data_requests
            .iter()
            .all(|head| head.contains("if-range: \"v1\"")),
        "every range must name the version it belongs to: {heads:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_server_without_ranges_is_never_split() {
    // Big enough that a split would be attempted, slow enough that the engine
    // gets a chance to attempt it.
    let data: Vec<u8> = (0..2 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let served = data.clone();
    let (url, requests) = spawn_slow_ignoring_server(served);
    let dir = temp_dir("no-ranges");
    let dest = dir.join("out.bin");
    let download = Download::new(54, url, dest.clone(), 8, None, None).expect("download");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    // Every question after the probe asks for the file from its first byte. A
    // range that starts later is answered with the whole file by such a server,
    // and that answer is not the range that was asked for.
    let heads = requests_so_far(&requests);
    assert!(
        heads.iter().all(|head| {
            head.lines()
                .find(|line| line.starts_with("range:"))
                .is_none_or(|line| line.trim().starts_with("range: bytes=0-"))
        }),
        "a server without ranges must not be split: {heads:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_range_the_server_does_not_have_fails_at_once() {
    let data = sample_data();
    let (url, requests) = spawn_scripted_server(move |head, seen| {
        if seen == 0 {
            let (start, end) = requested_range(head, data.len());
            return Some(partial_response(&data, start, end, ""));
        }
        Some(
            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */1024\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        )
    });
    let dir = temp_dir("416");
    let dest = dir.join("out.bin");
    let download = Download::new(53, url, dest.clone(), 1, None, None).expect("download");
    download.max_attempts.store(4, Ordering::SeqCst);

    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Failed);
    assert!(
        info.error.expect("a reason").contains("416"),
        "the reason names the answer"
    );
    assert_eq!(
        requests_so_far(&requests).len(),
        2,
        "a 416 is not asked for again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_unreadable_content_range_is_refused() {
    let data = sample_data();
    let (url, requests) = spawn_scripted_server(move |head, seen| {
        let (start, end) = requested_range(head, data.len());
        if seen == 0 {
            return Some(partial_response(&data, start, end, ""));
        }
        Some(
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: nonsense\r\nConnection: close\r\n\r\n",
                end - start + 1
            )
            .into_bytes(),
        )
    });
    let dir = temp_dir("bad-content-range");
    let dest = dir.join("out.bin");
    let download = Download::new(54, url, dest.clone(), 1, None, None).expect("download");

    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Failed);
    assert!(
        info.error.expect("a reason").contains("Content-Range"),
        "the reason names the header"
    );
    assert_eq!(
        requests_so_far(&requests).len(),
        2,
        "an answer that cannot be read is not asked for again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_short_answer_is_asked_for_again() {
    let data = sample_data();
    let served = data.clone();
    let (url, _requests) = spawn_scripted_server(move |head, seen| {
        let (start, end) = requested_range(head, served.len());
        if seen == 1 {
            // The headers promise the whole range, the body stops in the
            // middle of it.
            let mut response = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nConnection: close\r\n\r\n",
                end - start + 1,
                served.len()
            )
            .into_bytes();
            response.extend_from_slice(&served[start..start + 512]);
            return Some(response);
        }
        Some(partial_response(&served, start, end, ""))
    });
    let dir = temp_dir("short-answer");
    let dest = dir.join("out.bin");
    let download = Download::new(55, url, dest.clone(), 1, None, None).expect("download");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

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
