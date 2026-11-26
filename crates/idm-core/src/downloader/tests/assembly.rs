//! Joining the parts, replacing the destination and cleaning up afterwards.

use super::*;

#[tokio::test]
async fn a_file_of_several_ranges_is_joined_in_the_right_order() {
    // Two ranges, so the parts have to be joined in the right order.
    let data: Vec<u8> = (0..(2 * MIN_SPLIT)).map(|i| (i % 251) as u8).collect();
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
async fn a_file_that_was_not_allowed_to_be_replaced_is_left_alone() {
    let data = sample_data();
    let (url, _requests) = spawn_range_server(data);
    let dir = temp_dir("existing-destination");
    let dest = dir.join("out.bin");
    std::fs::write(&dest, b"old bytes").expect("the file that was already there");

    let download = Download::new(70, url, dest.clone(), 4, None, None).expect("download");
    run(silent(), Limits::default(), download.clone()).await;

    let info = download.info();
    assert_eq!(info.status, Status::Failed);
    assert!(
        info.error
            .expect("a reason")
            .contains("appeared while the download was running"),
        "the failure names the file that appeared"
    );
    assert_eq!(
        std::fs::read(&dest).expect("the old file"),
        b"old bytes",
        "a file the user did not agree to replace is never overwritten"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_confirmed_replace_puts_the_download_over_the_old_file() {
    let data = sample_data();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("confirmed-replace");
    let dest = dir.join("out.bin");
    std::fs::write(&dest, b"old bytes").expect("the file that was already there");

    let download = Download::new(71, url, dest.clone(), 4, None, None).expect("download");
    download.set_allow_replace(true);
    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("destination"), data);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A split can append a range whose start is before a range that was already
/// in the list. The parts are a file only when they are joined by where they
/// start, not by when they were planned.
#[tokio::test]
async fn parts_are_joined_in_file_order_not_plan_order() {
    let dir = temp_dir("assembly-order");
    let dest = dir.join("out.bin");
    let data = sample_data();
    let download = Download::new(
        80,
        "https://example.com/a.bin".to_string(),
        dest.clone(),
        4,
        None,
        None,
    )
    .expect("download");

    // The first part was split after the second one was already in the list,
    // so the list order is 0..300, 512..1024, 300..512.
    let ranges = [(0u64, 300u64), (512, 1024), (300, 512)];
    let mut segments = Vec::new();
    for (index, (start, end)) in ranges.iter().enumerate() {
        let part = segmenter::part_path(&dir, &download.stem, index as u32);
        std::fs::write(&part, &data[*start as usize..*end as usize]).expect("part file");
        segments.push(segmenter::Segment {
            id: index as u32,
            start: *start,
            end: *end,
            downloaded: end - start,
            part,
            status: SegmentStatus::Done,
        });
    }
    *download.segments.lock().expect("segments lock") = segments;
    download.total.store(data.len() as u64, Ordering::SeqCst);

    assemble(&download).await.expect("the assembly");
    assert_eq!(std::fs::read(&dest).expect("the file"), data);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discarding_progress_removes_the_parts_and_the_manifest() {
    let dir = temp_dir("discard-progress");
    let dest = dir.join("out.bin");
    let download = Download::new(
        72,
        "https://example.com/a.bin".to_string(),
        dest,
        4,
        None,
        None,
    )
    .expect("download");

    let mut segments = segmenter::plan(1024, 2, &dir, &download.stem);
    for segment in &mut segments {
        std::fs::write(&segment.part, vec![0u8; segment.len() as usize]).expect("part");
    }
    *download.segments.lock().expect("segments lock") = segments;
    download.total.store(1024, Ordering::SeqCst);
    download.save_manifest();
    assert!(download.manifest.exists(), "the download can resume");

    download.discard_progress().expect("discard");

    assert!(!download.manifest.exists(), "the resume state is gone");
    assert!(
        download.segments.lock().expect("segments lock").is_empty(),
        "the old ranges are forgotten"
    );
    assert_eq!(download.total.load(Ordering::SeqCst), 0);
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".part"))
        .collect();
    assert!(leftovers.is_empty(), "the parts are gone: {leftovers:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_row_that_leaves_the_list_takes_its_parts_with_it() {
    let dir = temp_dir("forget-progress");
    let dest = dir.join("out.bin");
    let download = Download::new(
        73,
        "https://example.com/a.bin".to_string(),
        dest,
        4,
        None,
        None,
    )
    .expect("download");

    let mut segments = segmenter::plan(1024, 2, &dir, &download.stem);
    for segment in &mut segments {
        std::fs::write(&segment.part, vec![0u8; segment.len() as usize]).expect("part");
    }
    *download.segments.lock().expect("segments lock") = segments;
    download.total.store(1024, Ordering::SeqCst);
    download.save_manifest();

    // The row is removed while its task is still winding down: the bytes go
    // now, and the task is told not to write them back when it ends.
    download.running.store(true, Ordering::SeqCst);
    download.forget_progress();

    assert!(!download.manifest.exists(), "the manifest is gone");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".part"))
        .collect();
    assert!(leftovers.is_empty(), "the parts are gone: {leftovers:?}");
    assert!(
        download.discarded.load(Ordering::SeqCst),
        "a winding down task cleans up instead of writing the state back"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The parts and the manifest live in the unfinished folder, never beside the
/// finished file in a category folder.
#[test]
fn an_unfinished_folder_keeps_the_resume_state_out_of_the_final_folder() {
    let dir = temp_dir("unfinished-folder");
    let final_folder = dir.join("Video");
    let work = dir.join("unfinished");
    let download = Download::new_in(
        73,
        "https://example.com/film.mp4".to_string(),
        final_folder.join("film.mp4"),
        Some(&work),
        2,
        None,
        None,
    )
    .expect("download");

    assert!(final_folder.is_dir(), "the finished folder is made");
    assert!(
        download.manifest.starts_with(&work),
        "the resume state is in the unfinished folder"
    );
    download.save_manifest();
    assert!(download.manifest.exists());
    assert!(
        !final_folder.read_dir().expect("read dir").any(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().contains(".idm"))
                .unwrap_or(false)
        }),
        "the final folder holds no temporary files"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A real transfer whose parts wait in the unfinished folder and whose bytes
/// end up in a category folder that did not exist when it started.
#[tokio::test]
async fn a_real_download_finishes_in_the_final_folder_and_leaves_nothing_behind() {
    let data = sample_data();
    let (url, _requests) = spawn_range_server(data.clone());
    let dir = temp_dir("unfinished-run");
    let unfinished = dir.join("unfinished");
    let final_folder = dir.join("Video");
    let dest = final_folder.join("film.bin");

    let download = Download::new_in(74, url, dest.clone(), Some(&unfinished), 4, None, None)
        .expect("download");
    assert!(
        final_folder.is_dir(),
        "the destination folder is made before the bytes arrive"
    );
    assert_eq!(download.dir, unfinished);
    assert!(
        download.manifest.starts_with(&unfinished),
        "the resume state waits in the unfinished folder"
    );

    run(silent(), Limits::default(), download.clone()).await;

    assert_eq!(download.info().status, Status::Completed);
    assert_eq!(std::fs::read(&dest).expect("the finished file"), data);

    let mut in_unfinished: Vec<String> = std::fs::read_dir(&unfinished)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    in_unfinished.sort();
    assert!(
        in_unfinished.is_empty(),
        "nothing of the transfer is left unfinished: {in_unfinished:?}"
    );
    assert!(
        !unfinished.exists(),
        "the folder the download owned goes with it"
    );
    let mut in_final: Vec<String> = std::fs::read_dir(&final_folder)
        .expect("read the final folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    in_final.sort();
    assert_eq!(in_final, vec!["film.bin".to_string()]);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A download whose parts live in a staging folder of their own takes that
/// folder with it when it leaves the list, and never takes the folder of the
/// user with it.
#[test]
fn a_staged_download_leaves_no_folder_behind_and_keeps_the_users_folder() {
    let dir = temp_dir("staged-folder");
    let staging = dir.join("staging").join("downloads").join("7");
    let final_folder = dir.join("Downloads");

    let download = Download::new_in(
        7,
        "https://example.com/film.mp4".to_string(),
        final_folder.join("film.mp4"),
        Some(&staging),
        2,
        None,
        None,
    )
    .expect("download");
    assert!(
        download.staged,
        "the folder of the parts is the application's"
    );

    std::fs::write(segmenter::part_path(&staging, &download.stem, 0), b"half").expect("part");
    download.save_manifest();
    assert!(download.manifest.exists());

    download.forget_progress();

    assert!(
        !staging.exists(),
        "the folder the download owned goes with it"
    );
    assert!(
        final_folder.is_dir(),
        "the folder of the user stays where it is"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A download whose parts sit beside the file keeps the user's folder: only
/// its own files are removed.
#[test]
fn a_download_beside_the_file_leaves_the_folder_alone() {
    let dir = temp_dir("beside-folder");
    let final_folder = dir.join("Downloads");
    std::fs::create_dir_all(&final_folder).expect("folder");

    let download = Download::new(
        8,
        "https://example.com/film.mp4".to_string(),
        final_folder.join("film.mp4"),
        2,
        None,
        None,
    )
    .expect("download");
    assert!(!download.staged, "the folder belongs to the user");

    std::fs::write(
        segmenter::part_path(&final_folder, &download.stem, 0),
        b"half",
    )
    .expect("part");
    download.save_manifest();

    download.forget_progress();

    assert!(final_folder.is_dir(), "the folder of the user stays");
    assert!(!final_folder.join("film.mp4.part0").exists());
    assert!(!download.manifest.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

/// While a download is running, the folder the reader chose holds nothing.
/// The bytes wait in the application's own storage; only the finished file
/// arrives at the destination, and a stopped download leaves nothing but its
/// resume state behind.
#[tokio::test]
async fn a_running_download_leaves_the_destination_folder_empty() {
    let data: Vec<u8> = (0..64 * 1024u32).map(|i| (i % 251) as u8).collect();
    let url = spawn_slow_server(data.clone());
    let dir = temp_dir("running-clean");
    let staging = dir.join("staging").join("downloads").join("9");
    let final_folder = dir.join("Downloads");
    let dest = final_folder.join("film.bin");

    let download =
        Download::new_in(9, url, dest.clone(), Some(&staging), 2, None, None).expect("download");
    let task = tokio::spawn(run(silent(), Limits::default(), download.clone()));

    // Let the transfer get going, then look at the folder of the reader.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let in_final: Vec<String> = std::fs::read_dir(&final_folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        in_final.is_empty(),
        "the folder of the reader holds nothing while the transfer runs: {in_final:?}"
    );
    assert!(
        !dest.exists(),
        "the file is not there before it is finished"
    );

    let in_staging: Vec<String> = std::fs::read_dir(&staging)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        in_staging.iter().any(|name| name.contains(".part")),
        "the bytes wait in the application's storage: {in_staging:?}"
    );

    download.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("a cancelled download must let its task go")
        .expect("the task must not panic");

    assert_eq!(download.info().status, Status::Cancelled);
    assert!(!dest.exists(), "a stopped download leaves no file behind");
    assert!(
        download.manifest.exists(),
        "the resume state is kept for the next start"
    );
    let after: Vec<String> = std::fs::read_dir(&final_folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        after.is_empty(),
        "the folder of the reader is still empty after the stop: {after:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
