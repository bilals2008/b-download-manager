use super::ebml::*;
use super::write::{ebml_header, info, track_entry};
use super::*;
use std::process::Command;

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
    let dir = std::env::temp_dir().join(format!("bdm-webm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test folder");
    dir
}

fn make(ffmpeg: &str, args: &[&str], out: &Path) -> bool {
    Command::new(ffmpeg)
        .args(args)
        .arg(out)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
fn joins_a_vp9_picture_and_an_opus_sound() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let dir = folder("vp9");
    let video = dir.join("video.track");
    let audio = dir.join("audio.track");
    // A build without libvpx/libopus skips this test rather than failing.
    let made_video = make(
        &ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=1",
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-f",
            "webm",
        ],
        &video,
    );
    let made_audio = make(
        &ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-vn",
            "-c:a",
            "libopus",
            "-f",
            "webm",
        ],
        &audio,
    );
    if !made_video || !made_audio {
        return;
    }

    let out = dir.join("joined.webm");
    join(&[video, audio], &out, &dir, Container::Webm).expect("a join");
    decodes(&ffmpeg, &out);
    let info = super::super::verify::inspect(&out).expect("the file is WebM");
    assert!(info.video_codec.is_some(), "the picture is there");
    assert!(info.audio_codec.is_some(), "the sound is there");
    let _ = std::fs::remove_dir_all(&dir);
}

/* -------------------------------------------- deterministic fixtures */

/// A whole WebM file, assembled from the same primitives the writer uses.
///
/// Building it here instead of with an encoder is what makes the reader's
/// edge cases testable everywhere: no ffmpeg, no sample files, no clock.
fn fixture(tracks: &[RawTrack], clusters: &[Vec<u8>]) -> Vec<u8> {
    let mut out = ebml_header("webm");
    out.extend(id(SEGMENT));
    out.extend(unknown_size());
    out.extend(info(0));
    let mut entries = Vec::new();
    for track in tracks {
        entries.extend(track_entry(track.number, track));
    }
    out.extend(element(TRACKS, &entries));
    for cluster in clusters {
        out.extend_from_slice(cluster);
    }
    out
}

fn video_track(number: u64) -> RawTrack {
    RawTrack {
        number,
        kind: 1,
        codec_id: b"V_VP9".to_vec(),
        default_duration: Some(40_000_000),
        pixel_width: Some(320),
        pixel_height: Some(240),
        ..RawTrack::default()
    }
}

fn audio_track(number: u64) -> RawTrack {
    RawTrack {
        number,
        kind: 2,
        codec_id: b"A_OPUS".to_vec(),
        default_duration: Some(20_000_000),
        codec_delay: Some(6_500_000),
        seek_pre_roll: Some(80_000_000),
        sample_rate: Some(48_000.0),
        channels: Some(2),
        ..RawTrack::default()
    }
}

fn simple_block(track: u64, relative: i16, keyframe: bool, frames: &[u8]) -> Vec<u8> {
    let mut payload = encode_vint(track);
    payload.extend(relative.to_be_bytes());
    payload.push(if keyframe { 0x80 } else { 0x00 });
    payload.extend(frames);
    element(SIMPLE_BLOCK, &payload)
}

fn block_group(track: u64, relative: i16, keyframe: bool, padding: i64, frames: &[u8]) -> Vec<u8> {
    let mut payload = encode_vint(track);
    payload.extend(relative.to_be_bytes());
    // A plain `Block` must not set the keyframe bit: inside a group the
    // absence of a `ReferenceBlock` is what says the frame stands alone.
    payload.push(0x00);
    payload.extend(frames);
    let mut body = element(BLOCK, &payload);
    if !keyframe {
        body.extend(uint_element(REFERENCE_BLOCK, 0));
    }
    body.extend(element(DISCARD_PADDING, &padding.to_be_bytes()));
    element(BLOCK_GROUP, &body)
}

fn cluster(at_ms: u64, children: &[Vec<u8>], sized: bool) -> Vec<u8> {
    let mut body = uint_element(CLUSTER_TIMESTAMP, at_ms);
    for child in children {
        body.extend_from_slice(child);
    }
    let mut out = id(CLUSTER);
    out.extend(if sized {
        encode_size(body.len() as u64)
    } else {
        unknown_size()
    });
    out.extend(body);
    out
}

fn found(bytes: &[u8], wanted: u64) -> Option<usize> {
    let pattern = id(wanted);
    bytes
        .windows(pattern.len())
        .position(|window| window == pattern.as_slice())
}

/// The body of the first `wanted` element of a finished file.
fn body_of(bytes: &[u8], wanted: u64) -> Option<Vec<u8>> {
    let at = found(bytes, wanted)?;
    let (_, id_len) = read_id(&bytes[at..])?;
    let (size, size_len) = read_size(&bytes[at + id_len..])?;
    let data = at + id_len + size_len;
    Some(bytes[data..data + size?].to_vec())
}

/// Where the Segment's data begins, which is what a cue position counts from.
fn segment_start(bytes: &[u8]) -> usize {
    let (_, ebml_id) = read_id(bytes).expect("the EBML header");
    let (ebml_size, ebml_len) = read_size(&bytes[ebml_id..]).expect("its size");
    let after = ebml_id + ebml_len + ebml_size.expect("a known size");
    let (_, segment_id) = read_id(&bytes[after..]).expect("the Segment");
    let (_, segment_len) = read_size(&bytes[after + segment_id..]).expect("its size");
    after + segment_id + segment_len
}

fn written(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("the fixture");
    path
}

#[test]
fn a_padding_is_a_signed_number() {
    assert_eq!(read_int(&[0x85]), Some(-123));
    assert_eq!(
        read_int(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x85]),
        Some(-123),
        "the top bit is what makes it negative"
    );
    assert_eq!(read_int(&[0x00, 0x7B]), Some(123));
    assert_eq!(read_int(&[]), None);
    assert_eq!(
        read_int(&[0; 9]),
        None,
        "a Matroska integer holds eight bytes"
    );
}

#[test]
fn a_block_group_keeps_its_padding_and_its_reference() {
    let dir = folder("padding");
    let input = written(
        &dir,
        "audio.track",
        &fixture(
            &[audio_track(1)],
            &[cluster(
                0,
                &[
                    block_group(1, 0, true, -123, &[1, 2]),
                    block_group(1, 20, false, 456, &[3, 4]),
                ],
                true,
            )],
        ),
    );
    let out = dir.join("joined.webm");
    join(&[input], &out, &dir, Container::Webm).expect("a join");

    let joined = read_input(&out).expect("the joined file");
    assert_eq!(joined.blocks.len(), 2, "both frames are kept");
    assert_eq!(joined.blocks[0].discard_padding, Some(-123));
    assert_eq!(joined.blocks[1].discard_padding, Some(456));
    assert!(
        joined.blocks[0].keyframe,
        "a padded first frame still stands alone"
    );
    assert!(
        !joined.blocks[1].keyframe,
        "a reference block is not a keyframe"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_size_cluster_does_not_swallow_the_clusters_after_it() {
    let dir = folder("nested");
    let input = written(
        &dir,
        "video.track",
        &fixture(
            &[video_track(1)],
            &[
                cluster(0, &[simple_block(1, 0, true, &[1])], false),
                cluster(2000, &[simple_block(1, 0, true, &[2])], true),
            ],
        ),
    );
    let out = dir.join("joined.webm");
    join(&[input], &out, &dir, Container::Webm).expect("a join");

    let joined = read_input(&out).expect("the joined file");
    assert_eq!(
        joined.blocks.len(),
        2,
        "the blocks after the open cluster are kept"
    );
    assert_eq!(joined.blocks[1].time_ns, 2_000_000_000);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_track_that_never_appears_is_not_declared() {
    let dir = folder("unused-track");
    let input = written(
        &dir,
        "mixed.track",
        &fixture(
            &[video_track(1), audio_track(2)],
            &[cluster(0, &[simple_block(1, 0, true, &[1])], true)],
        ),
    );
    let out = dir.join("joined.webm");
    join(&[input], &out, &dir, Container::Webm).expect("a join");

    let joined = read_input(&out).expect("the joined file");
    assert_eq!(
        joined.tracks.len(),
        1,
        "a stream that never delivers a frame is not promised"
    );
    assert_eq!(joined.tracks[0].number, 1);
    assert_eq!(joined.tracks[0].kind, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_codec_delay_and_the_seek_pre_roll_survive_the_join() {
    let dir = folder("delay");
    let input = written(
        &dir,
        "audio.track",
        &fixture(
            &[audio_track(1)],
            &[cluster(0, &[simple_block(1, 0, true, &[1])], true)],
        ),
    );
    let out = dir.join("joined.webm");
    join(&[input], &out, &dir, Container::Webm).expect("a join");

    let joined = read_input(&out).expect("the joined file");
    assert_eq!(
        joined.tracks[0].codec_delay,
        Some(6_500_000),
        "without this the sound starts ahead of the picture"
    );
    assert_eq!(joined.tracks[0].seek_pre_roll, Some(80_000_000));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_file_ends_when_its_last_frame_ends() {
    let dir = folder("duration");
    let input = written(
        &dir,
        "video.track",
        &fixture(
            &[video_track(1)],
            &[cluster(
                0,
                &[
                    simple_block(1, 0, true, &[1]),
                    simple_block(1, 1000, true, &[2]),
                ],
                true,
            )],
        ),
    );
    let out = dir.join("joined.webm");
    join(&[input], &out, &dir, Container::Webm).expect("a join");

    let bytes = std::fs::read(&out).expect("the file");
    let info = body_of(&bytes, INFO).expect("an Info");
    let duration = element_float(&info, DURATION).expect("a duration");
    // The last frame starts at one second and is forty milliseconds long.
    assert!(
        (duration - 1040.0).abs() < 0.5,
        "the duration was {duration}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_cue_point_lands_on_the_cluster_it_names() {
    let dir = folder("cues");
    let input = written(
        &dir,
        "video.track",
        &fixture(
            &[video_track(1)],
            &[
                cluster(0, &[simple_block(1, 0, true, &[1])], true),
                cluster(2000, &[simple_block(1, 0, true, &[2])], true),
            ],
        ),
    );
    let out = dir.join("joined.webm");
    join(&[input], &out, &dir, Container::Webm).expect("a join");

    let bytes = std::fs::read(&out).expect("the file");
    let cues = body_of(&bytes, CUES).expect("a cue index");
    let start = segment_start(&bytes);
    let mut points = 0;
    for point in elements(&cues) {
        if point.id != CUE_POINT {
            continue;
        }
        let entry = &cues[point.data..point.end];
        let positions = element_body(entry, CUE_TRACK_POSITIONS).expect("track positions");
        let position = element_uint(positions, CUE_CLUSTER_POSITION).expect("a position");
        let at = start + position as usize;
        assert_eq!(
            &bytes[at..at + id(CLUSTER).len()],
            id(CLUSTER).as_slice(),
            "a cue must name the first byte of a cluster"
        );
        assert_eq!(element_uint(positions, CUE_TRACK), Some(1));
        points += 1;
    }
    assert_eq!(points, 2, "one cue per keyframe cluster");
    let _ = std::fs::remove_dir_all(&dir);
}
