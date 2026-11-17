//! Running a download: pause, cancel, state, metadata and the FTP source.

use super::*;

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
fn a_download_stopped_with_a_reason_says_why() {
    let dir = temp_dir("fail-with");
    let dest = dir.join("out.bin");
    let download = Download::new(
        90,
        "https://example.com/a.bin".to_string(),
        dest,
        1,
        None,
        None,
    )
    .expect("download");
    download.set_status(Status::Running);

    download.fail_with("the network adapter VPN is not connected");

    let info = download.info();
    assert_eq!(info.status, Status::Failed);
    assert_eq!(
        info.error.as_deref(),
        Some("the network adapter VPN is not connected")
    );
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
fn the_category_and_note_are_trimmed_and_kept_short() {
    let dir = temp_dir("meta");
    let download = Download::new(
        24,
        "https://example.com/a.bin".to_string(),
        dir.join("out.bin"),
        2,
        None,
        None,
    )
    .expect("download");

    download.set_meta(
        Some("  Video  ".to_string()),
        Some("  a note  ".to_string()),
    );
    let info = download.info();
    assert_eq!(info.category.as_deref(), Some("Video"));
    assert_eq!(info.description.as_deref(), Some("a note"));

    download.set_meta(Some("   ".to_string()), Some("x".repeat(600)));
    let info = download.info();
    assert_eq!(info.category, None, "an empty category is no category");
    assert_eq!(info.description.expect("a note").chars().count(), 500);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_category_can_change_without_touching_the_note() {
    let dir = temp_dir("category-only");
    let download = Download::new(
        25,
        "https://example.com/a.bin".to_string(),
        dir.join("out.bin"),
        2,
        None,
        None,
    )
    .expect("download");

    download.set_meta(
        Some("Video".to_string()),
        Some("keep this note".to_string()),
    );
    download.set_category(Some("  Music  ".to_string()));
    let info = download.info();
    assert_eq!(info.category.as_deref(), Some("Music"));
    assert_eq!(info.description.as_deref(), Some("keep this note"));

    download.set_category(None);
    let info = download.info();
    assert_eq!(info.category, None, "no category is a valid assignment");
    assert_eq!(info.description.as_deref(), Some("keep this note"));

    download.set_category(Some("x".repeat(60)));
    assert_eq!(
        download
            .info()
            .category
            .expect("a category")
            .chars()
            .count(),
        40
    );

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
        category: Some("Programs".to_string()),
        description: Some("the installer".to_string()),
        replace: true,
        unfinished_dir: None,
    };

    let download = Download::from_known(&known).expect("download");
    assert!(
        download.known().replace,
        "the Replace answer outlives the process"
    );
    let info = download.info();
    assert_eq!(info.speed_limit, 32 * 1024, "the cap survives a restart");
    assert_eq!(info.category.as_deref(), Some("Programs"));
    assert_eq!(info.description.as_deref(), Some("the installer"));
    assert_eq!(info.status, Status::Paused);
    assert_eq!(
        info.segments.len(),
        0,
        "only a manifest can describe the ranges"
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
