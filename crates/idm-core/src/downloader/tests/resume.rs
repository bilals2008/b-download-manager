//! The resume state: manifests, changed files and moving the state on disk.

use super::*;

#[tokio::test]
async fn a_resume_of_a_file_that_changed_starts_over() {
    let data = sample_data();
    let served = data.clone();
    let (url, requests) = spawn_scripted_server(move |head, _| {
        let (start, end) = requested_range(head, served.len());
        Some(partial_response(&served, start, end, "ETag: \"new\"\r\n"))
    });
    let dir = temp_dir("changed-validator");
    let dest = dir.join("out.bin");
    let download = Download::new(51, url, dest.clone(), 1, None, None).expect("download");

    // The plan on disk belongs to the version the server served before.
    let mut segments = segmenter::plan(data.len() as u64, 1, &dir, &download.stem);
    segments[0].downloaded = 400;
    std::fs::write(&segments[0].part, &data[..400]).expect("part file");
    let manifest = Manifest {
        url: download.url.clone(),
        total: data.len() as u64,
        segments,
        validator: Some("\"old\"".to_string()),
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let heads = requests_so_far(&requests);
    assert!(
        heads
            .iter()
            .any(|head| head.contains("range: bytes=0-1023")),
        "the bytes of the old version must be fetched again: {heads:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_changed_size_starts_the_file_over_instead_of_failing() {
    let old: Vec<u8> = sample_data();
    let new: Vec<u8> = sample_data()
        .iter()
        .chain(sample_data().iter())
        .copied()
        .collect();
    let served_old = old.clone();
    let served_new = new.clone();
    let (url, requests) = spawn_scripted_server(move |head, seen| {
        match seen {
            // First question: the old copy, 1024 bytes.
            0 => {
                let (start, end) = requested_range(head, served_old.len());
                Some(partial_response(&served_old, start, end, "ETag: \"v1\"\r\n"))
            }
            // The range is answered from a copy that is twice as big: the
            // bytes on disk belong to a file that is not there any more.
            1 => Some(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: 0\r\nContent-Range: bytes 0-1023/2048\r\nConnection: close\r\n\r\n"
                    .as_bytes()
                    .to_vec(),
            ),
            // The restart asks again: the new copy, 2048 bytes.
            _ => {
                let (start, end) = requested_range(head, served_new.len());
                Some(partial_response(&served_new, start, end, "ETag: \"v2\"\r\n"))
            }
        }
    });
    let dir = temp_dir("changed-size");
    let dest = dir.join("out.bin");
    let download = Download::new(52, url, dest.clone(), 1, None, None).expect("download");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(
        std::fs::read(&dest).expect("destination"),
        new,
        "the bytes of the old copy are not mixed into the new one"
    );
    assert!(
        requests_so_far(&requests).len() >= 4,
        "the download asks the server again before it starts over"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_unknown_size_download_resumes_from_a_validator() {
    let data = sample_data();
    let served = data.clone();
    let (url, requests) = spawn_scripted_server(move |head, seen| {
        let (start, end) = requested_range(head, served.len());
        if seen == 0 {
            // The probe: the server does not know the size of the file.
            let mut probe = b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/*\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n".to_vec();
            probe.extend_from_slice(&served[..1]);
            return Some(probe);
        }
        let mut response = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/*\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
            end - start + 1
        )
        .into_bytes();
        response.extend_from_slice(&served[start..=end]);
        Some(response)
    });
    let dir = temp_dir("unknown-resume");
    let dest = dir.join("out.bin");
    let download = Download::new(57, url, dest.clone(), 2, None, None).expect("download");

    // The first run got 400 bytes of a file whose end it never learned.
    let segments = segmenter::plan_unknown(&dir, &download.stem);
    std::fs::write(&segments[0].part, &data[..400]).expect("part file");
    let mut segments = segments;
    segments[0].downloaded = 400;
    let manifest = Manifest {
        url: download.url.clone(),
        total: 0,
        segments,
        validator: Some("\"v1\"".to_string()),
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Completed, "error: {:?}", info.error);
    assert_eq!(info.resumable, Some(true));
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let heads = requests_so_far(&requests);
    let resumed = heads
        .iter()
        .find(|head| head.contains("range: bytes=400-"))
        .expect("the range must continue where the first run stopped");
    assert!(
        resumed.contains("if-range: \"v1\""),
        "the resume must name the version it continues: {resumed}"
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
        validator: None,
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

/// A split appends the tail of a range to the end of the list, so the ranges
/// of a manifest are not in file order. Such a manifest must still be resumed
/// from: it used to be thrown away, and the download started over from zero.
#[tokio::test]
async fn a_manifest_whose_ranges_are_not_in_file_order_is_resumed_from() {
    let data = sample_data();
    let (url, requests) = spawn_range_server(data.clone());
    let dir = temp_dir("resume-out-of-order");
    let dest = dir.join("out.bin");
    let download = Download::new(81, url, dest.clone(), 4, None, None).expect("download");

    // The first range was split after the other two were already in the list,
    // so the list order is 0..300, 512..1024, 300..512.
    let ranges = [(0u64, 300u64, 300u64), (512, 1024, 0), (300, 512, 0)];
    let mut segments = Vec::new();
    for (index, (start, end, downloaded)) in ranges.iter().enumerate() {
        let part = segmenter::part_path(&dir, &download.stem, index as u32);
        if *downloaded > 0 {
            std::fs::write(&part, &data[*start as usize..*end as usize]).expect("part file");
        }
        segments.push(Segment {
            id: index as u32,
            start: *start,
            end: *end,
            downloaded: *downloaded,
            part,
            status: if *downloaded > 0 {
                SegmentStatus::Done
            } else {
                SegmentStatus::Pending
            },
        });
    }
    let manifest = Manifest {
        url: download.url.clone(),
        total: data.len() as u64,
        segments,
        validator: None,
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let heads = requests_so_far(&requests);
    assert!(
        heads.iter().any(|head| head.contains("range: bytes=300-")),
        "the download must continue where it stopped: {heads:?}"
    );
    // The only request for the first byte is the probe that asks the server
    // for the size of the file.
    let fetched: Vec<&String> = heads
        .iter()
        .filter(|head| !head.contains("range: bytes=0-0"))
        .collect();
    assert!(
        !fetched.iter().any(|head| head.contains("range: bytes=0-")),
        "the bytes that were already there must not be fetched again: {heads:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A crash can leave a plan with ranges that never started. Their part files
/// were never created, and that must not throw away the bytes of the ranges
/// that did run.
#[tokio::test]
async fn a_resume_keeps_the_ranges_whose_parts_were_never_created() {
    let data = sample_data();
    let (url, requests) = spawn_range_server(data.clone());
    let dir = temp_dir("resume-missing-part");
    let dest = dir.join("out.bin");
    let download = Download::new(18, url, dest.clone(), 2, None, None).expect("download");

    let first = Segment {
        id: 0,
        start: 0,
        end: 600,
        downloaded: 600,
        part: segmenter::part_path(&dir, &download.stem, 0),
        status: SegmentStatus::Done,
    };
    std::fs::write(&first.part, &data[..600]).expect("part file");
    let second = Segment {
        id: 1,
        start: 600,
        end: data.len() as u64,
        downloaded: 0,
        part: segmenter::part_path(&dir, &download.stem, 1),
        status: SegmentStatus::Pending,
    };
    assert!(!second.part.exists(), "the second range never ran");
    let manifest = Manifest {
        url: download.url.clone(),
        total: data.len() as u64,
        segments: vec![first, second],
        validator: None,
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(
        download.info().status,
        Status::Completed,
        "error: {:?}",
        download.info().error
    );
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let heads = requests_so_far(&requests);
    assert!(
        heads
            .iter()
            .any(|head| head.contains("range: bytes=600-1023")),
        "the range that never ran must be fetched: {heads:?}"
    );
    assert!(
        !heads
            .iter()
            .any(|head| head.contains("range: bytes=0-1023")),
        "the bytes that were already there must not be fetched again: {heads:?}"
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
        validator: None,
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

/// A restart: the row comes back from the index, finds its parts and its
/// manifest in the unfinished folder and asks for the bytes it has not got.
#[tokio::test]
async fn a_download_restored_from_the_unfinished_folder_resumes_where_it_stopped() {
    let data = sample_data();
    let (url, requests) = spawn_range_server(data.clone());
    let dir = temp_dir("unfinished-resume");
    let unfinished = dir.join("unfinished");
    let final_folder = dir.join("Programs");
    let dest = final_folder.join("setup.bin");

    let download = Download::new_in(75, url, dest.clone(), Some(&unfinished), 4, None, None)
        .expect("download");

    // The first run got 400 bytes in and then the application was closed.
    let mut segments = segmenter::plan(data.len() as u64, 1, &unfinished, &download.stem);
    segments[0].downloaded = 400;
    std::fs::write(&segments[0].part, &data[..400]).expect("part file");
    let manifest = Manifest {
        url: download.url.clone(),
        total: data.len() as u64,
        segments,
        validator: None,
    };
    std::fs::write(
        &download.manifest,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write manifest");

    // What the index remembers about it, and what the next start reads back.
    let known = download.known();
    assert_eq!(
        known.unfinished_dir.as_deref(),
        Some(unfinished.to_string_lossy().as_ref())
    );
    assert_eq!(
        crate::session::inspect(&known),
        crate::session::Recovered::Resumable,
        "the manifest in the unfinished folder is enough to resume"
    );
    let restored = Download::from_known(&known).expect("a resumable row");
    assert_eq!(restored.dir, unfinished);
    assert!(restored.manifest.starts_with(&unfinished));

    run(silent(), Limits::default(), restored.clone()).await;

    assert_eq!(restored.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("the finished file"), data);
    let heads: Vec<String> = std::iter::from_fn(|| requests.try_recv().ok())
        .map(|head| head.to_ascii_lowercase())
        .collect();
    assert!(
        heads.iter().any(|head| head.contains("range: bytes=400-")),
        "the restored download must not ask for the bytes it has: {heads:?}"
    );
    assert!(
        !final_folder.join("setup.bin.idm.json").exists(),
        "the manifest never sits in the finished folder"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The resume state of a download can be moved into another folder, and the
/// manifest that records it travels with the bytes it describes.
#[test]
fn the_resume_state_can_be_moved_into_another_folder() {
    let dir = temp_dir("relocate");
    let old_folder = dir.join("Downloads");
    let new_folder = dir.join("staging").join("downloads").join("7");
    std::fs::create_dir_all(&old_folder).expect("folder");

    let part = segmenter::part_path(&old_folder, "film.mp4", 0);
    std::fs::write(&part, b"12345678").expect("part");
    let manifest_path = old_folder.join("film.mp4.idm.json");
    let manifest = Manifest {
        url: "https://example.com/film.mp4".to_string(),
        total: 16,
        segments: vec![Segment {
            id: 0,
            start: 0,
            end: 16,
            downloaded: 8,
            part: part.clone(),
            status: SegmentStatus::Pending,
        }],
        validator: None,
    };
    std::fs::write(
        &manifest_path,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write the manifest");

    let moved = relocate_resume_state(&manifest_path, &new_folder).expect("a move");

    assert!(
        manifest_path.exists(),
        "the old manifest stays until the index points at the new folder"
    );
    assert!(part.exists(), "and so do the old bytes");
    let moved_manifest = new_folder.join("film.mp4.idm.json");
    assert!(
        moved_manifest.is_file(),
        "the manifest is in the new folder"
    );
    assert_eq!(
        std::fs::read(new_folder.join("film.mp4.part0")).expect("the moved part"),
        b"12345678"
    );

    let read: Manifest =
        serde_json::from_str(&std::fs::read_to_string(&moved_manifest).expect("read back"))
            .expect("the manifest reads back");
    assert_eq!(
        read.segments[0].downloaded, 8,
        "the bytes are still recorded"
    );
    assert_eq!(
        read.segments[0].part,
        new_folder.join("film.mp4.part0"),
        "the part path points at the new folder"
    );

    // The index is written here; only then does the old state go.
    discard_relocated(&moved);
    assert!(!manifest_path.exists());
    assert!(!part.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

/// A move that cannot be checked leaves everything where it was: a recorded
/// range without its bytes is a broken state, not a state to move.
#[test]
fn a_move_without_the_bytes_it_records_refuses() {
    let dir = temp_dir("relocate-missing");
    let old_folder = dir.join("Downloads");
    let new_folder = dir.join("staging").join("downloads").join("7");
    std::fs::create_dir_all(&old_folder).expect("folder");

    let part = segmenter::part_path(&old_folder, "film.mp4", 0);
    let manifest_path = old_folder.join("film.mp4.idm.json");
    let manifest = Manifest {
        url: "https://example.com/film.mp4".to_string(),
        total: 16,
        segments: vec![Segment {
            id: 0,
            start: 0,
            end: 16,
            downloaded: 8,
            part: part.clone(),
            status: SegmentStatus::Pending,
        }],
        validator: None,
    };
    std::fs::write(
        &manifest_path,
        serde_json::to_string(&manifest).expect("manifest"),
    )
    .expect("write the manifest");

    let error = relocate_resume_state(&manifest_path, &new_folder).expect_err("a refusal");
    assert!(error.contains("missing"), "the error was: {error}");
    assert!(manifest_path.exists(), "the resume state was not destroyed");

    let _ = std::fs::remove_dir_all(&dir);
}
