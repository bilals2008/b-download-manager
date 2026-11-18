use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bdm-media-persist-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn started_job(id: u64, dest: &Path) -> MediaJob {
    let mut job = MediaJob::new(
        id,
        "https://example.org/watch".to_string(),
        "https://cdn.example.org/master.m3u8".to_string(),
        MediaKindHint::Hls,
        "Film".to_string(),
        "https://example.org/watch".to_string(),
        vec![("cookie".to_string(), "session=secret".to_string())],
        Some("1080p".to_string()),
    );
    job.state = MediaState::Ready;
    job.dest = Some(dest.to_string_lossy().to_string());
    job.request = Some(DownloadRequest {
        video: "1080p".to_string(),
        audio: None,
        name: "Film 1080p.mp4".to_string(),
        dest: dest.parent().unwrap().to_string_lossy().to_string(),
        subtitle: None,
        subtitle_file: None,
        audio_format: None,
    });
    job
}

#[test]
fn a_started_job_round_trips_without_its_headers() {
    let dir = temp_dir("round-trip");
    let job = started_job(7, &dir.join("Film 1080p.mp4"));
    let record = KnownMediaJob::from_job(&job).expect("a started job is history");
    let index = MediaIndex::new(dir.join("media.json"));
    index.save(std::slice::from_ref(&record)).expect("save");

    let text = std::fs::read_to_string(dir.join("media.json")).expect("the file");
    assert!(!text.contains("session=secret"), "headers never reach disk");

    let loaded = index.load();
    assert_eq!(loaded.len(), 1);
    let restored = loaded.into_iter().next().unwrap().into_job();
    assert_eq!(restored.id, 7);
    assert_eq!(restored.state, MediaState::Ready);
    assert_eq!(restored.dest, job.dest);
    assert_eq!(
        restored.request.as_ref().map(|r| r.video.as_str()),
        Some("1080p")
    );
    assert!(restored.headers.is_empty());
    assert!(!restored.downloading);
    assert!(restored.control.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_job_that_was_only_parsed_is_not_history() {
    let job = MediaJob::new(
        1,
        String::new(),
        "https://cdn.example.org/a.m3u8".to_string(),
        MediaKindHint::Auto,
        "Offer".to_string(),
        String::new(),
        Vec::new(),
        None,
    );
    assert!(KnownMediaJob::from_job(&job).is_none());
}

#[test]
fn one_broken_record_does_not_take_the_history_down() {
    let dir = temp_dir("broken-record");
    let job = started_job(3, &dir.join("Film.mp4"));
    let record = KnownMediaJob::from_job(&job).expect("history");
    let mut values = serde_json::to_value(vec![record]).expect("json");
    values
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "id": "not a number" }));
    let path = dir.join("media.json");
    std::fs::write(&path, serde_json::to_string(&values).unwrap()).expect("write");

    let loaded = MediaIndex::new(path).load();
    assert_eq!(loaded.len(), 1, "the readable record survives");
    assert_eq!(loaded[0].id, 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_or_broken_index_is_an_empty_index() {
    let dir = temp_dir("missing");
    assert!(MediaIndex::new(dir.join("nothing.json")).load().is_empty());
    let path = dir.join("media.json");
    std::fs::write(&path, "{ not json").expect("write");
    assert!(MediaIndex::new(path).load().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn folder_bytes_counts_the_tracks_on_disk() {
    let dir = temp_dir("bytes");
    std::fs::write(dir.join("video.seg0"), vec![0u8; 100]).expect("write");
    std::fs::create_dir_all(dir.join("parts")).expect("folder");
    std::fs::write(dir.join("parts").join("video.seg1"), vec![0u8; 50]).expect("write");
    assert_eq!(folder_bytes(&dir), 150);
    assert_eq!(folder_bytes(&dir.join("nowhere")), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
