use super::*;

#[tokio::test]
#[ignore = "reaches YouTube"]
async fn a_live_page_resolves_to_direct_tracks() {
    let (manifest, source) = resolve("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "Video")
        .await
        .expect("a resolution");
    assert!(!manifest.formats.is_empty(), "the page offered formats");
    match source {
        SourcePlan::Files { tracks } => {
            assert!(
                tracks.iter().any(|t| t.format.kind == MediaKind::Video),
                "a picture"
            );
            assert!(
                tracks.iter().all(|t| t.url().starts_with("http")),
                "direct urls"
            );
        }
        _ => panic!("files expected"),
    }
}
