use super::client::{ask_player, vr_profile};
use super::parse::{digits_after, plan_from, text_between};
use super::*;
use idm_core::media::MediaKind;

#[tokio::test]
#[ignore = "reaches YouTube and downloads"]
async fn the_vr_client_resolves_and_downloads_a_track() {
    let id = video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").expect("a video id");
    let client = reqwest::Client::builder().build().expect("client");
    let page = client
        .get(format!("https://www.youtube.com/watch?v={id}&hl=en&gl=US"))
        .header("user-agent", DESKTOP_UA)
        .header("cookie", "CONSENT=YES+1")
        .send()
        .await
        .expect("page");
    let html = page.text().await.expect("html");
    let visitor = text_between(&html, "\"VISITOR_DATA\":\"", '"').expect("visitor");
    let sts = digits_after(&html, "\"STS\":").unwrap_or(DEFAULT_STS);
    let data = ask_player(&client, &id, visitor, sts, &vr_profile(visitor))
        .await
        .expect("the VR answer");
    let planned = plan_from(&data, "Video", &vr_profile(visitor)).expect("a plan");
    assert!(
        !planned.manifest.formats.is_empty(),
        "the VR answer offered formats"
    );
    let track = match planned.source {
        SourcePlan::Files { tracks } => tracks
            .into_iter()
            .filter(|t| t.format.kind == MediaKind::Video)
            .min_by_key(|t| t.format.size.unwrap_or(u64::MAX))
            .filter(|t| t.format.size.unwrap_or(0) > 1_000_000)
            .expect("a video track"),
        _ => panic!("files expected"),
    };
    let dir = std::env::temp_dir().join(format!("bdm-yt-vr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let dest = dir.join("video.track");
    let outcome = idm_core::media::download::download_track(
        idm_core::limits::Limits::default(),
        idm_core::media::download::TrackSource {
            init: None,
            segments: vec![idm_core::media::TrackSegment {
                url: track.url().to_string(),
                range: None,
            }],
            dest: dest.clone(),
            work_dir: dir.clone(),
            stem: "video".to_string(),
            connections: 4,
            proxy: None,
            referer: None,
            headers: track
                .lease
                .user_agent
                .iter()
                .map(|user_agent| ("user-agent".to_string(), (*user_agent).to_string()))
                .collect(),
            header_origin: None,
            fingerprint: Some(track.lease.representation.key()),
        },
        None,
        None,
    )
    .await;
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "vr itag {} size {:?} outcome {:?} bytes {}",
        track.format.id, track.format.size, outcome, size
    );
    assert!(size > 1_000_000, "a whole track");
    let _ = std::fs::remove_dir_all(&dir);
}
