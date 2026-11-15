use super::deliver::{deliver, DeliverError};
use super::*;
use idm_core::media::MediaKind;

#[test]
fn the_extension_follows_the_container() {
    let mut format = Format::new("v", MediaKind::Video, "mp4");
    assert_eq!(output_container(&format), "mp4");
    format.container = "ts".to_string();
    assert_eq!(output_container(&format), "ts");
    format.container = "webm".to_string();
    assert_eq!(output_container(&format), "webm");
    format.container = "m4a".to_string();
    assert_eq!(output_container(&format), "m4a");
}

#[test]
fn a_name_gets_one_extension() {
    assert_eq!(with_extension("Film", "mp4"), "Film.mp4");
    assert_eq!(with_extension("Film.mp4", "mp4"), "Film.mp4");
    assert_eq!(with_extension("Film.old.mp4", "mp4"), "Film.old.mp4");
    assert_eq!(with_extension("Film.old", "mp4"), "Film.old.mp4");
    assert_eq!(with_extension("Film.ts", "mp4"), "Film.mp4");
    assert_eq!(with_extension("Dr. No", "mp4"), "Dr. No.mp4");
}

#[test]
fn a_component_cannot_leave_the_folder() {
    assert_eq!(safe_component("a/b\\c:d"), "a_b_c_d");
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bdm-deliver-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test folder");
    dir
}

fn delivery(extension: &str) -> Delivery {
    Delivery {
        extension: extension.to_string(),
        container: None,
        video: true,
        audio: false,
        audio_selected: false,
        ffmpeg: None,
        captions: None,
        convert: None,
    }
}

#[test]
fn a_file_that_is_already_the_destination_is_left_where_it_is() {
    let dir = scratch("in-place");
    let dest = dir.join("Film.webm");
    std::fs::write(&dest, super::super::tools::tiny_webm()).expect("the file");

    // The track and the destination are the same path: a download resumed
    // straight into the folder it belongs to.
    let saved = deliver(&dest, vec![dest.clone()], &dir, &delivery("webm"), &|_| {})
        .expect("a file that checks out");

    assert_eq!(saved, dest);
    assert!(dest.is_file(), "the destination is still there");
    assert_eq!(
        std::fs::read(&dest).expect("the file"),
        super::super::tools::tiny_webm()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_does_not_check_out_never_replaces_the_destination() {
    let dir = scratch("check");
    let dest = dir.join("Film.webm");
    let old = b"the file that was already there";
    std::fs::write(&dest, old).expect("the old file");

    // A WebM header with no segment and no track: it is the container the
    // name promises, and it holds nothing a person could watch.
    let track = dir.join("video.track");
    std::fs::write(
        &track,
        super::super::tools::ebml(
            &[0x1A, 0x45, 0xDF, 0xA3],
            &super::super::tools::ebml(&[0x42, 0x82], b"webm"),
        ),
    )
    .expect("the track");

    let failed = match deliver(&dest, vec![track], &dir, &delivery("webm"), &|_| {}) {
        Ok(_) => panic!("a file with no picture cannot be saved"),
        Err(error) => error,
    };
    assert!(matches!(failed, DeliverError::Failed(_)));
    assert_eq!(
        std::fs::read(&dest).expect("the old file"),
        old,
        "the destination the user had is untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_join_that_fails_never_replaces_the_destination() {
    let dir = scratch("join");
    let dest = dir.join("Film.mp4");
    let old = b"the file that was already there";
    std::fs::write(&dest, old).expect("the old file");

    // Text where an MP4 should be: the join refuses it before anything is
    // moved into the destination.
    let track = dir.join("video.track");
    std::fs::write(&track, b"<!doctype html><title>Not found</title>").expect("the track");

    let failed = match deliver(&dest, vec![track], &dir, &delivery("mp4"), &|_| {}) {
        Ok(_) => panic!("an HTML page is not a video"),
        Err(error) => error,
    };
    assert!(matches!(failed, DeliverError::Failed(_)));
    assert_eq!(
        std::fs::read(&dest).expect("the old file"),
        old,
        "the destination the user had is untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
