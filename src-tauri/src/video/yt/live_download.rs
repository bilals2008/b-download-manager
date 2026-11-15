use super::*;
#[tokio::test]
#[ignore = "reaches YouTube and downloads"]
async fn a_live_video_track_downloads_whole() {
    let (manifest, source) = resolve("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "Video")
        .await
        .expect("resolve");
    let track = match source {
        SourcePlan::Files { tracks } => tracks
            .into_iter()
            .filter(|t| {
                t.format.kind == MediaKind::Video && t.format.size.unwrap_or(0) > 20_000_000
            })
            .min_by_key(|t| t.format.size.unwrap_or(u64::MAX))
            .expect("a video track"),
        _ => panic!("files expected"),
    };
    eprintln!("itag {} size {:?}", track.format.id, track.format.size);
    let dir = std::env::temp_dir().join(format!("bdm-yt-dl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let dest = dir.join("video.track");
    let src = idm_core::media::download::TrackSource {
        init: None,
        segments: vec![idm_core::media::TrackSegment {
            url: track.url().to_string(),
            range: None,
        }],
        dest: dest.clone(),
        work_dir: dir.clone(),
        stem: "video".to_string(),
        connections: 8,
        proxy: None,
        referer: Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string()),
        headers: track
            .lease
            .user_agent
            .iter()
            .map(|user_agent| ("user-agent".to_string(), (*user_agent).to_string()))
            .collect(),
        header_origin: None,
        fingerprint: Some(track.lease.representation.key()),
    };
    let start = std::time::Instant::now();
    let outcome = idm_core::media::download::download_track(
        idm_core::limits::Limits::default(),
        src,
        None,
        None,
    )
    .await
    .expect("download");
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "outcome {:?} bytes {} in {:?}",
        outcome,
        size,
        start.elapsed()
    );
    assert!(size > 1_000_000, "a whole track");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = manifest;
}
