use super::client::{ios_profile, vr_profile};
use super::parse::{
    check_playable, codec_of, digits_after, merge_failures, plan_from, player_version, text_between,
};
use super::resolver::programs;
use super::*;

fn answer() -> Value {
    json!({
        "playabilityStatus": { "status": "OK" },
        "videoDetails": { "videoId": "dQw4w9WgXcQ", "title": "A song", "lengthSeconds": "212" },
        "streamingData": {
            "adaptiveFormats": [
                {
                    "itag": 137,
                    "mimeType": "video/mp4; codecs=\"avc1.640028\"",
                    "bitrate": 4000000,
                    "contentLength": "24000000",
                    "height": "1080",
                    "url": "https://rr1.googlevideo.com/videoplayback?itag=137&expire=1893456000&sig=secret"
                },
                {
                    "itag": 140,
                    "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                    "bitrate": 128000,
                    "contentLength": "3400000",
                    "url": "https://rr2.googlevideo.com/videoplayback?itag=140&expire=1893456000&sig=secret"
                }
            ]
        }
    })
}

#[test]
fn a_watch_page_names_its_video() {
    assert_eq!(
        video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1").as_deref(),
        Some("dQw4w9WgXcQ")
    );
    assert_eq!(
        video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
    assert_eq!(
        video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").as_deref(),
        Some("dQw4w9WgXcQ")
    );
    assert!(video_id("https://example.org/watch?v=x").is_none());
}

#[test]
fn a_non_youtube_page_is_not_resolved() {
    assert!(is_youtube_page(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
    assert!(!is_youtube_page("https://example.org/watch?v=dQw4w9WgXcQ"));
    assert!(!is_youtube_page("https://www.youtube.com/"));
}

#[test]
fn a_page_value_is_read_without_a_regex() {
    let html = r#"{"VISITOR_DATA":"Cgtabc","STS":20759}"#;
    assert_eq!(
        text_between(html, "\"VISITOR_DATA\":\"", '"'),
        Some("Cgtabc")
    );
    assert_eq!(digits_after(html, "\"STS\":"), Some(20759));
}

#[test]
fn a_fallback_client_names_itself() {
    let ios = ios_profile("Cgtabc");
    let vr = vr_profile("Cgtabc");
    assert_eq!(ios.player_client["clientName"], "iOS");
    assert!(ios.client_name.is_some());
    assert_eq!(vr.player_client["clientName"], "ANDROID_VR");
    assert_eq!(vr.player_client["visitorData"], "Cgtabc");
    assert!(vr.client_name.is_none());
}

#[test]
fn an_answer_becomes_leases_not_bare_addresses() {
    let planned = plan_from(&answer(), "Fallback", &ios_profile("Cgtabc")).expect("a plan");
    assert!(!planned.throttled);
    let tracks = match planned.source {
        SourcePlan::Files { tracks } => tracks,
        _ => panic!("files expected"),
    };
    assert_eq!(tracks.len(), 2);
    let video = tracks
        .iter()
        .find(|track| track.format.id == "137")
        .expect("a picture");
    assert_eq!(video.lease.authority, Authority::Native);
    assert_eq!(video.lease.client, "iOS");
    assert_eq!(video.lease.user_agent.as_deref(), Some(IOS_UA));
    assert!(video.lease.expires_at.is_some(), "the expire says when");
    assert_eq!(video.lease.representation.id, "137");
    assert_eq!(video.lease.representation.bytes, Some(24_000_000));
    assert_eq!(
        video.lease.representation.codec.as_deref(),
        Some("avc1.640028")
    );
    // The signed value is not in anything a log would print.
    assert!(!video.lease.describe().contains("secret"));
}

#[test]
fn an_answer_about_another_video_is_refused() {
    let mut data = answer();
    data["videoDetails"]["videoId"] = json!("somethingElse");
    assert_eq!(
        check_playable(&data, "dQw4w9WgXcQ"),
        Err(YtError::Identity {
            asked: "dQw4w9WgXcQ".to_string(),
            answered: "somethingElse".to_string()
        })
    );
}

#[test]
fn a_refusal_keeps_the_players_reason() {
    let data = json!({
        "playabilityStatus": { "status": "UNPLAYABLE", "reason": "This video is private" }
    });
    let error = check_playable(&data, "x").expect_err("a refusal");
    assert!(
        error.to_string().contains("This video is private"),
        "{error}"
    );
    assert!(error.to_string().contains("UNPLAYABLE"), "{error}");
}

#[test]
fn a_licence_is_named_as_protection_not_as_a_generic_failure() {
    let mut data = answer();
    data["streamingData"]["drmParams"] = json!("a licence");
    assert!(matches!(
        check_playable(&data, "dQw4w9WgXcQ"),
        Err(YtError::Protected(_))
    ));
}

#[test]
fn sabr_only_is_its_own_answer() {
    let data = json!({
        "playabilityStatus": { "status": "OK" },
        "videoDetails": { "videoId": "x" },
        "streamingData": { "serverAbrStreamingUrl": "https://www.youtube.com/api" }
    });
    assert_eq!(
        plan_from(&data, "x", &ios_profile("v")).err(),
        Some(YtError::NoPlainAddress)
    );
}

#[test]
fn a_throttled_answer_is_kept_but_marked() {
    let mut data = answer();
    data["streamingData"]["adaptiveFormats"][0]["url"] =
        json!("https://rr1.googlevideo.com/videoplayback?itag=137&n=AbC");
    let planned = plan_from(&data, "x", &ios_profile("v")).expect("a plan");
    assert!(planned.throttled);
    let tracks = match planned.source {
        SourcePlan::Files { tracks } => tracks,
        _ => panic!("files expected"),
    };
    assert!(tracks[0].lease.note.is_some());
}

#[test]
fn the_player_script_names_the_identity_a_transform_belongs_to() {
    let mut data = answer();
    data["playerConfig"] = json!({
        "assets": { "js": "/s/player/1a2b3c4d/player_ias.vflset/en_US/base.js" }
    });
    assert_eq!(player_version(&data).as_deref(), Some("1a2b3c4d"));

    data["playerConfig"]["assets"]["js"] = json!("https://example.org/elsewhere.js");
    assert_eq!(player_version(&data), None);
    data["playerConfig"]["assets"]["js"] = json!("/s/player/..%2Fetc/player_ias.js");
    assert_eq!(player_version(&data), None);
    assert_eq!(player_version(&answer()), None);
}

#[test]
fn a_learned_transform_is_applied_only_to_its_own_player() {
    // This client name belongs to this test alone, so a parallel test's
    // program cannot be the one that answers.
    let platform = PlayerIdentity::new("TEST-REWRITE", "1a2b3c4d");
    programs().lock().expect("the cache").put(
        platform,
        challenge::Program::parse("reverse").expect("a program"),
    );

    let mut data = answer();
    data["playerConfig"] = json!({
        "assets": { "js": "/s/player/1a2b3c4d/player_ias.vflset/en_US/base.js" }
    });
    data["streamingData"]["adaptiveFormats"][0]["url"] =
        json!("https://rr1.googlevideo.com/videoplayback?itag=137&n=abc");
    let mut profile = ios_profile("v");
    profile.label = "TEST-REWRITE";
    let planned = plan_from(&data, "x", &profile).expect("a plan");
    assert!(!planned.throttled, "the transform removed the throttling");
    let tracks = match planned.source {
        SourcePlan::Files { tracks } => tracks,
        _ => panic!("files expected"),
    };
    assert!(tracks[0].url().contains("n=cba"), "{}", tracks[0].url());
    assert!(tracks[0].url().contains("itag=137"));

    // The same answer from a player this build never learned keeps its
    // address untouched and stays marked as throttled.
    data["playerConfig"]["assets"]["js"] =
        json!("/s/player/9z9z9z9z/player_ias.vflset/en_US/base.js");
    let planned = plan_from(&data, "x", &profile).expect("a plan");
    assert!(planned.throttled);
    let tracks = match planned.source {
        SourcePlan::Files { tracks } => tracks,
        _ => panic!("files expected"),
    };
    assert!(tracks[0].url().contains("n=abc"), "{}", tracks[0].url());
}

#[test]
fn the_codec_is_read_out_of_the_mime_type() {
    assert_eq!(
        codec_of("video/mp4; codecs=\"avc1.640028\"").as_deref(),
        Some("avc1.640028")
    );
    assert_eq!(codec_of("audio/webm").as_deref(), None);
    assert_eq!(codec_of("video/mp4; codecs=\"\"").as_deref(), None);
}

#[test]
fn every_client_failing_names_every_client() {
    let error = merge_failures(vec![
        YtError::Client {
            client: "iOS".to_string(),
            reason: "refused".to_string(),
        },
        YtError::NoStreams,
    ]);
    let text = error.to_string();
    assert!(text.contains("refused"), "{text}");
    assert!(text.contains("no downloadable stream"), "{text}");
}
