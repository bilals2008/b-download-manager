use super::boxes::boxed;
use super::fragment::fragment_samples;
use super::read::{composition_offsets, moov_tracks};
use super::write::{build_moov, ctts, edts};
use super::*;
use std::process::Command;

/// ffmpeg is a test tool here, never a runtime dependency. A machine
/// without it skips the real-media tests rather than failing.
fn ffmpeg() -> Option<String> {
    for name in ["ffmpeg", "ffmpeg.exe"] {
        if Command::new(name)
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Some(name.to_string());
        }
    }
    None
}

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bdm-mp4-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test folder");
    dir
}

fn make(ffmpeg: &str, args: &[&str], out: &Path) {
    let status = Command::new(ffmpeg)
        .args(args)
        .arg(out)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg produced {out:?} with {args:?}");
}

fn decodes(ffmpeg: &str, path: &Path) {
    let status = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "the joined file must decode");
}

#[test]
fn joins_a_fragmented_picture_and_a_progressive_sound() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let dir = folder("frag");
    let video = dir.join("video.track");
    let audio = dir.join("audio.track");
    make(
        &ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=2",
            "-an",
            "-c:v",
            "mpeg4",
            "-movflags",
            "frag_keyframe+empty_moov",
            "-f",
            "mp4",
        ],
        &video,
    );
    make(
        &ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-vn",
            "-c:a",
            "aac",
            "-f",
            "mp4",
        ],
        &audio,
    );

    let out = dir.join("joined.mp4");
    join(&[video, audio], &out, &dir, Container::Mp4).expect("a join");
    decodes(&ffmpeg, &out);

    let info = super::super::verify::inspect(&out).expect("the file is an MP4");
    assert_eq!(info.video_codec.as_deref(), Some("mp4v"));
    assert_eq!(info.audio_codec.as_deref(), Some("mp4a"));
    assert!(info.duration.unwrap_or(0.0) > 1.0, "the joined file lasts");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rewrites_a_progressive_picture_alone() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let dir = folder("prog");
    let video = dir.join("video.track");
    make(
        &ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=2",
            "-an",
            "-c:v",
            "mpeg4",
            "-f",
            "mp4",
        ],
        &video,
    );
    let out = dir.join("joined.mp4");
    join(&[video], &out, &dir, Container::Mp4).expect("a join");
    decodes(&ffmpeg, &out);
    let info = super::super::verify::inspect(&out).expect("the file is an MP4");
    assert_eq!(info.video_codec.as_deref(), Some("mp4v"));
    let _ = std::fs::remove_dir_all(&dir);
}

/* ------------------------------------------------ deterministic boxes */
//
// The tests below build the boxes by hand, so they state exactly which
// layout each one refuses and never depend on a tool being installed.

/// A full-box header: one version byte and three flag bytes.
fn full(version: u8, flags: u32) -> [u8; 4] {
    let flags = flags.to_be_bytes();
    [version, flags[1], flags[2], flags[3]]
}

/// A `tfhd` stating the track, its default duration and its default size.
fn tfhd(track_id: u32, duration: u32, size: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(full(0, 0x18)); // default-duration + default-size present
    body.extend(track_id.to_be_bytes());
    body.extend(duration.to_be_bytes());
    body.extend(size.to_be_bytes());
    boxed(b"tfhd", &body)
}

/// A `trun` naming each sample's duration and size, with an optional
/// data offset.
fn trun(offset: Option<i32>, sizes: &[u32]) -> Vec<u8> {
    let mut flags = 0x100 | 0x200; // per-sample duration + size
    if offset.is_some() {
        flags |= 0x1;
    }
    let mut body = Vec::new();
    body.extend(full(0, flags));
    body.extend((sizes.len() as u32).to_be_bytes());
    if let Some(offset) = offset {
        body.extend(offset.to_be_bytes());
    }
    for size in sizes {
        body.extend(&100u32.to_be_bytes()); // duration
        body.extend(size.to_be_bytes());
    }
    boxed(b"trun", &body)
}

/// A sample that only states its timing.
fn sample_at(decode_time: u64, composition_offset: i32) -> Sample {
    Sample {
        offset: decode_time,
        size: 10,
        duration: 100,
        decode_time,
        composition_offset,
        sync: true,
    }
}

/// A track with valid-enough headers and no samples, for the writer tests.
fn bare_track(handler: [u8; 4]) -> Track {
    let mut tkhd = vec![0u8; 24];
    tkhd[12..16].copy_from_slice(&1u32.to_be_bytes());
    let mut mdhd = vec![0u8; 20];
    mdhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    Track {
        source: PathBuf::new(),
        handler,
        timescale: 1000,
        tkhd,
        mdhd,
        stsd: Vec::new(),
        samples: Vec::new(),
        start_time: 0,
    }
}

#[test]
fn every_sample_run_of_a_fragment_is_read() {
    // A fragment may split its samples into several runs. Reading only the
    // first one would silently drop the rest of every fragment.
    let moof = boxed(
        b"traf",
        &[
            tfhd(1, 100, 10),
            trun(Some(0), &[10, 10]),
            trun(None, &[10, 10]),
        ]
        .concat(),
    );
    let fragments = [Fragment { moof, base: 1000 }];
    let samples = fragment_samples(&fragments, &[], 1).expect("the fragment");
    assert_eq!(samples.len(), 4, "both runs are read");
    let offsets: Vec<u64> = samples.iter().map(|sample| sample.offset).collect();
    // The second run carries no offset of its own, so it continues where
    // the first one stopped.
    assert_eq!(offsets, vec![1000, 1010, 1020, 1030]);
}

#[test]
fn a_sample_run_that_ends_early_is_an_error_not_a_crash() {
    let mut body = Vec::new();
    body.extend(full(0, 0x200));
    body.extend(4u32.to_be_bytes()); // four samples promised
    body.extend(10u32.to_be_bytes()); // room for one
    let moof = boxed(b"traf", &[tfhd(1, 100, 10), boxed(b"trun", &body)].concat());
    let fragments = [Fragment { moof, base: 0 }];
    let error = fragment_samples(&fragments, &[], 1).expect_err("a truncated run");
    assert!(matches!(error, MuxError::Io(_)), "{error:?}");
}

#[test]
fn a_fragment_that_begins_late_keeps_its_decode_time() {
    let mut base = Vec::new();
    base.extend(full(1, 0));
    base.extend(900_000u64.to_be_bytes());
    let moof = boxed(
        b"traf",
        &[tfhd(1, 100, 10), boxed(b"tfdt", &base), trun(None, &[10])].concat(),
    );
    let fragments = [Fragment { moof, base: 0 }];
    let samples = fragment_samples(&fragments, &[], 1).expect("the fragment");
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].decode_time, 900_000, "the fragment's own clock");
}

#[test]
fn the_joined_movie_numbers_its_tracks_once_each() {
    // Two sources very often each call their only track `1`.
    let moov = build_moov(&[bare_track(*b"vide"), bare_track(*b"soun")]);
    let ids: Vec<u32> = moov_tracks(&moov[8..])
        .expect("the built index")
        .into_iter()
        .map(|track| track.id)
        .collect();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn both_versions_of_the_composition_table_are_read() {
    // Version 0 states the offset unsigned, version 1 signed. A B-frame
    // stream needs the signed form.
    let mut signed = Vec::new();
    signed.extend(full(1, 0));
    signed.extend(1u32.to_be_bytes()); // one entry
    signed.extend(3u32.to_be_bytes()); // covering three samples
    signed.extend((-1000i32).to_be_bytes());
    assert_eq!(
        composition_offsets(Some(&signed), 3).expect("the table"),
        Some(vec![-1000, -1000, -1000])
    );

    let mut unsigned = Vec::new();
    unsigned.extend(full(0, 0));
    unsigned.extend(1u32.to_be_bytes());
    unsigned.extend(2u32.to_be_bytes());
    unsigned.extend(500u32.to_be_bytes());
    assert_eq!(
        composition_offsets(Some(&unsigned), 2).expect("the table"),
        Some(vec![500, 500])
    );
}

#[test]
fn a_composition_table_that_cannot_be_read_is_refused() {
    // Two entries promised, one delivered.
    let mut short = Vec::new();
    short.extend(full(1, 0));
    short.extend(2u32.to_be_bytes());
    short.extend(1u32.to_be_bytes());
    short.extend(0i32.to_be_bytes());
    assert!(
        composition_offsets(Some(&short), 2).is_err(),
        "a table shorter than it says"
    );

    // A version this build does not know is refused rather than guessed.
    let mut later = Vec::new();
    later.extend(full(2, 0));
    later.extend(0u32.to_be_bytes());
    assert!(
        composition_offsets(Some(&later), 1).is_err(),
        "a version this build cannot read"
    );

    // Runs that overrun the sample table would move a picture that is not
    // there.
    let mut overrun = Vec::new();
    overrun.extend(full(1, 0));
    overrun.extend(1u32.to_be_bytes());
    overrun.extend(5u32.to_be_bytes());
    overrun.extend(0i32.to_be_bytes());
    assert!(
        composition_offsets(Some(&overrun), 2).is_err(),
        "runs that name more samples than exist"
    );

    // Runs that fall short would leave trailing samples with no offset.
    let mut underrun = Vec::new();
    underrun.extend(full(1, 0));
    underrun.extend(1u32.to_be_bytes());
    underrun.extend(1u32.to_be_bytes());
    underrun.extend(0i32.to_be_bytes());
    assert!(
        composition_offsets(Some(&underrun), 3).is_err(),
        "a table that does not reach every sample"
    );
}

#[test]
fn a_reordering_track_keeps_its_composition_offsets() {
    // The decode order I, P, B, B presents as I, B, B, P.
    let mut track = bare_track(*b"vide");
    track.samples = vec![
        sample_at(0, 0),
        sample_at(100, 200),
        sample_at(200, -100),
        sample_at(300, -100),
    ];
    assert!(track.reorders());
    assert_eq!(track.first_presentation(), 0, "the first picture leads");

    let written = ctts(&track);
    assert_eq!(&written[4..8], b"ctts");
    let offsets = composition_offsets(Some(&written[8..]), 4)
        .expect("the table")
        .expect("offsets");
    assert_eq!(offsets, vec![0, 200, -100, -100], "the offsets survive");
}

#[test]
fn an_edit_list_appears_only_when_the_source_started_late() {
    let mut early = bare_track(*b"vide");
    early.samples = vec![sample_at(0, 0)];
    assert!(
        edts(&early).is_empty(),
        "a source that starts at zero needs no edit"
    );

    let mut late = bare_track(*b"vide");
    late.samples = vec![sample_at(90_000, 0)];
    late.start_time = 90_000;
    let edit = edts(&late);
    assert!(!edit.is_empty(), "a source that starts at 90 s needs one");
    assert!(
        edit.windows(4).any(|window| window == b"elst"),
        "the edit list is inside an edts"
    );
}
