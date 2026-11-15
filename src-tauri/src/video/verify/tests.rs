use super::*;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bdm-verify-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test folder");
    dir
}

/// A minimal MP4: `ftyp`, a `moov` with one video track and one sound
/// track, and an empty `mdat`. The bytes are built here so the parser is
/// tested without a real file.
fn sample_mp4(duration: u32) -> Vec<u8> {
    fn atom(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend(((body.len() + 8) as u32).to_be_bytes());
        out.extend(tag);
        out.extend(body);
        out
    }
    fn mvhd(duration: u32) -> Vec<u8> {
        let mut body = vec![0u8; 100];
        body[12..16].copy_from_slice(&1000u32.to_be_bytes());
        body[16..20].copy_from_slice(&duration.to_be_bytes());
        atom(b"mvhd", &body)
    }
    fn hdlr(kind: &[u8; 4]) -> Vec<u8> {
        let mut body = vec![0u8; 25];
        body[8..12].copy_from_slice(kind);
        atom(b"hdlr", &body)
    }
    fn stsd(format: &[u8; 4]) -> Vec<u8> {
        let mut entry = vec![0u8; 16];
        entry[4..8].copy_from_slice(format);
        let mut body = vec![0u8; 8];
        body[4..8].copy_from_slice(&1u32.to_be_bytes());
        body.extend(entry);
        atom(b"stsd", &body)
    }
    fn trak(kind: &[u8; 4], format: &[u8; 4]) -> Vec<u8> {
        let stbl = atom(b"stbl", &stsd(format));
        let minf = atom(b"minf", &stbl);
        let media = atom(b"mdia", &[hdlr(kind), minf].concat());
        atom(b"trak", &media)
    }
    let moov = atom(
        b"moov",
        &[
            mvhd(duration),
            trak(b"vide", b"avc1"),
            trak(b"soun", b"mp4a"),
        ]
        .concat(),
    );
    let mut ftyp = Vec::new();
    ftyp.extend(b"isom");
    ftyp.extend(0u32.to_be_bytes());
    ftyp.extend(b"isomiso2avc1mp41");
    let mut out = atom(b"ftyp", &ftyp);
    out.extend(moov);
    out.extend(atom(b"mdat", &[0u8; 32]));
    out
}

#[test]
fn an_mp4_index_names_its_tracks_and_duration() {
    let dir = temp_dir("mp4");
    let path = dir.join("film.mp4");
    std::fs::write(&path, sample_mp4(10_000)).expect("the file");

    let info = inspect(&path).expect("an inspection");
    assert_eq!(info.container, MediaContainer::Mp4);
    assert_eq!(info.duration, Some(10.0));
    assert_eq!(info.video_codec.as_deref(), Some("avc1"));
    assert_eq!(info.audio_codec.as_deref(), Some("mp4a"));

    // The playback check is exercised against real media in
    // `tests/media_pipeline.rs`; this fixture has an index but no samples.
    let info = inspect(&path).expect("the same file read twice");
    assert_eq!(info.video_codec.as_deref(), Some("avc1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_picture_is_refused() {
    // A join test may have pointed `BDM_FFMPEG` at a stand-in.
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("nopicture");
    let path = dir.join("film.mp4");
    let mut bytes = sample_mp4(5_000);
    // Break the video track's handler into a text track.
    let at = bytes
        .windows(4)
        .position(|window| window == b"vide")
        .expect("the handler");
    bytes[at..at + 4].copy_from_slice(b"text");
    std::fs::write(&path, bytes).expect("the file");

    let error = check_finished(
        &path,
        Expect {
            extension: "mp4",
            video: true,
            audio: false,
            audio_selected: false,
        },
    )
    .expect_err("no picture");
    assert!(error.contains("picture"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_selected_audio_track_that_is_lost_is_refused_but_a_silent_source_is_kept() {
    // A join test may have pointed `BDM_FFMPEG` at a stand-in.
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("silent");
    let path = dir.join("film.mp4");
    let mut bytes = sample_mp4(5_000);
    // Break the sound track's handler into a text track: the source itself
    // carries no audio, which is a silent clip and not a broken download.
    let at = bytes
        .windows(4)
        .position(|window| window == b"soun")
        .expect("the audio handler");
    bytes[at..at + 4].copy_from_slice(b"text");
    std::fs::write(&path, bytes).expect("the file");

    let kept = check_finished(
        &path,
        Expect {
            extension: "mp4",
            video: true,
            audio: true,
            audio_selected: false,
        },
    )
    .err()
    .unwrap_or_default();
    // The fixture has no samples, so the playback check refuses it; what
    // matters is that no sound is not what refused it.
    assert!(!kept.contains("no sound"), "{kept}");

    // The same file is a broken download when a separate audio track was
    // fetched to be joined onto it.
    let error = check_finished(
        &path,
        Expect {
            extension: "mp4",
            video: true,
            audio: true,
            audio_selected: true,
        },
    )
    .expect_err("a lost audio track");
    assert!(error.contains("no sound"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_html_page_named_mp4_is_not_a_video() {
    // A join test may have pointed `BDM_FFMPEG` at a stand-in.
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("html");
    let path = dir.join("film.mp4");
    std::fs::write(&path, b"<!doctype html><title>Not found</title>").expect("the file");

    let info = inspect(&path).expect("an inspection");
    assert_eq!(info.container, MediaContainer::Unknown);
    let error = check_finished(
        &path,
        Expect {
            extension: "mp4",
            video: true,
            audio: false,
            audio_selected: false,
        },
    )
    .expect_err("not a video");
    assert!(error.contains("not a playable video file"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_with_almost_no_time_is_refused() {
    // A join test may have pointed `BDM_FFMPEG` at a stand-in. Ten
    // milliseconds is stated by the index, so the refusal does not depend
    // on whether a decoder happens to read the sample.
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("noduration");
    let path = dir.join("film.mp4");
    std::fs::write(&path, sample_mp4(10)).expect("the file");
    let error = check_finished(
        &path,
        Expect {
            extension: "mp4",
            video: true,
            audio: false,
            audio_selected: false,
        },
    )
    .expect_err("almost no time");
    assert!(error.contains("no time long"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_mp4_has_no_index() {
    let dir = temp_dir("truncated");
    let path = dir.join("film.mp4");
    let mut bytes = sample_mp4(5_000);
    bytes.truncate(40);
    std::fs::write(&path, bytes).expect("the file");
    let error = inspect(&path).expect_err("no index");
    assert!(error.contains("index"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_adts_stream_is_recognised_and_is_not_an_mp4() {
    let dir = temp_dir("adts");
    let path = dir.join("sound.aac");
    // Two ADTS frames with a silent payload.
    let mut bytes = Vec::new();
    for _ in 0..2 {
        bytes.extend([0xFF, 0xF1, 0x50, 0x80, 0x01, 0x1F, 0xFC]);
        bytes.extend([0u8; 100]);
    }
    std::fs::write(&path, bytes).expect("the file");
    let info = inspect(&path).expect("an inspection");
    assert_eq!(info.container, MediaContainer::Adts);
    assert!(!MediaContainer::Adts.fits_extension("m4a"));
    assert!(MediaContainer::Adts.fits_extension("aac"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_matroska_header_is_refused() {
    let dir = temp_dir("matroska");
    let path = dir.join("film.mkv");
    std::fs::write(&path, [0x1A, 0x45, 0xDF, 0xA3, 0x00]).expect("the file");
    let error = inspect(&path).expect_err("no header");
    assert!(!error.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_extension_only_accepts_what_it_can_hold() {
    assert!(MediaContainer::Mp4.fits_extension("m4a"));
    assert!(MediaContainer::Mp4.fits_extension("MP4"));
    assert!(!MediaContainer::Mp4.fits_extension("webm"));
    assert!(MediaContainer::Webm.fits_extension("mkv"));
    assert!(MediaContainer::Matroska.fits_extension("webm"));
    assert!(!MediaContainer::MpegTs.fits_extension("mp4"));
}

/* ------------------------------------------- deep index checks */

fn boxed(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend(((body.len() + 8) as u32).to_be_bytes());
    out.extend(tag);
    out.extend(body);
    out
}

/// A complete, self-consistent MP4: two fifty-byte samples in one chunk,
/// one of them a keyframe, two seconds long, with a decoder configuration.
///
/// The chunk offset is a parameter because it can only be known once the
/// boxes in front of the `mdat` have been measured.
fn indexed_mp4(offset: u64) -> Vec<u8> {
    let mut stts = 0u32.to_be_bytes().to_vec();
    stts.extend(1u32.to_be_bytes());
    stts.extend(2u32.to_be_bytes());
    stts.extend(1000u32.to_be_bytes());
    let mut stsc = 0u32.to_be_bytes().to_vec();
    stsc.extend(1u32.to_be_bytes());
    stsc.extend(1u32.to_be_bytes());
    stsc.extend(2u32.to_be_bytes());
    stsc.extend(1u32.to_be_bytes());
    let mut stsz = 0u32.to_be_bytes().to_vec();
    stsz.extend(0u32.to_be_bytes());
    stsz.extend(2u32.to_be_bytes());
    stsz.extend(50u32.to_be_bytes());
    stsz.extend(50u32.to_be_bytes());
    let mut co64 = 0u32.to_be_bytes().to_vec();
    co64.extend(1u32.to_be_bytes());
    co64.extend(offset.to_be_bytes());
    let mut stss = 0u32.to_be_bytes().to_vec();
    stss.extend(1u32.to_be_bytes());
    stss.extend(1u32.to_be_bytes());

    let mut entry = vec![0u8; 16];
    let configuration = boxed(b"avcC", &[1, 2, 3, 4]);
    entry[0..4].copy_from_slice(&((16 + configuration.len()) as u32).to_be_bytes());
    entry[4..8].copy_from_slice(b"avc1");
    entry.extend(configuration);
    let mut stsd = 0u32.to_be_bytes().to_vec();
    stsd.extend(1u32.to_be_bytes());
    stsd.extend(entry);

    let stbl = boxed(
        b"stbl",
        &[
            boxed(b"stsd", &stsd),
            boxed(b"stts", &stts),
            boxed(b"stsc", &stsc),
            boxed(b"stsz", &stsz),
            boxed(b"co64", &co64),
            boxed(b"stss", &stss),
        ]
        .concat(),
    );
    let mut mdhd = vec![0u8; 24];
    mdhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mdhd[16..20].copy_from_slice(&2000u32.to_be_bytes());
    let mut hdlr = vec![0u8; 25];
    hdlr[8..12].copy_from_slice(b"vide");
    let mdia = boxed(
        b"mdia",
        &[
            boxed(b"mdhd", &mdhd),
            boxed(b"hdlr", &hdlr),
            boxed(b"minf", &stbl),
        ]
        .concat(),
    );

    let mut mvhd = vec![0u8; 100];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&2000u32.to_be_bytes());
    let moov = boxed(
        b"moov",
        &[boxed(b"mvhd", &mvhd), boxed(b"trak", &mdia)].concat(),
    );

    let mut ftyp = Vec::new();
    ftyp.extend(b"isom");
    ftyp.extend(0u32.to_be_bytes());
    ftyp.extend(b"isomiso2avc1mp41");
    let mut out = boxed(b"ftyp", &ftyp);
    out.extend(moov);
    out.extend(boxed(b"mdat", &[0u8; 100]));
    out
}

/// The same file, with its chunk offset pointing at the real payload.
fn complete_mp4() -> Vec<u8> {
    let probe = indexed_mp4(0);
    let offset = probe.len() as u64 - 100;
    indexed_mp4(offset)
}

fn place(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("the file");
    path
}

fn picture() -> Expect<'static> {
    Expect {
        extension: "mp4",
        video: true,
        audio: false,
        audio_selected: false,
    }
}

#[test]
fn a_complete_index_is_verified_against_the_file() {
    let dir = temp_dir("indexed");
    let path = place(&dir, "film.mp4", &complete_mp4());

    let info = inspect(&path).expect("an inspection");
    assert_eq!(info.playback, Playback::Verified);
    assert_eq!(info.sample_count, 2, "both samples are accounted for");

    let checked = check_finished(&path, picture()).expect("a good file");
    assert_eq!(checked.duration, Some(2.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sample_that_reaches_past_the_end_of_the_file_is_refused() {
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("short-mdat");
    // The index still names a hundred bytes of samples; only sixty remain.
    let mut bytes = complete_mp4();
    bytes.truncate(bytes.len() - 40);
    let path = place(&dir, "film.mp4", &bytes);

    let info = inspect(&path).expect("an inspection");
    assert!(
        matches!(&info.playback, Playback::Broken(reason) if reason.contains("past the end")),
        "{:?}",
        info.playback
    );
    let error = check_finished(&path, picture()).expect_err("a truncated download");
    assert!(error.contains("past the end"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_keyframe_that_is_not_a_sample_is_refused() {
    let dir = temp_dir("keyframe");
    let mut bytes = complete_mp4();
    let at = bytes
        .windows(4)
        .position(|window| window == b"stss")
        .expect("the keyframe table");
    // The table names sample 99 of a track that has two. The first entry
    // follows version/flags and the count.
    bytes[at + 12..at + 16].copy_from_slice(&99u32.to_be_bytes());
    let path = place(&dir, "film.mp4", &bytes);

    let info = inspect(&path).expect("an inspection");
    assert!(
        matches!(&info.playback, Playback::Broken(reason) if reason.contains("keyframe")),
        "{:?}",
        info.playback
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_timing_table_that_names_another_count_is_refused() {
    let dir = temp_dir("timing");
    let mut bytes = complete_mp4();
    let at = bytes
        .windows(4)
        .position(|window| window == b"stts")
        .expect("the timing table");
    // Three samples of timing for a track whose sample table has two. The
    // run count is the first field of the body, after version/flags.
    bytes[at + 8..at + 12].copy_from_slice(&3u32.to_be_bytes());
    let path = place(&dir, "film.mp4", &bytes);

    let info = inspect(&path).expect("an inspection");
    assert!(
        matches!(&info.playback, Playback::Broken(reason) if reason.contains("timing table")),
        "{:?}",
        info.playback
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_size_table_shorter_than_its_own_count_is_refused() {
    let dir = temp_dir("sizes");
    let mut bytes = complete_mp4();
    let at = bytes
        .windows(4)
        .position(|window| window == b"stsz")
        .expect("the size table");
    // Four samples counted, two sizes given. The count lives at body byte
    // eight: version/flags, the uniform size, then the count.
    bytes[at + 12..at + 16].copy_from_slice(&4u32.to_be_bytes());
    let path = place(&dir, "film.mp4", &bytes);

    let info = inspect(&path).expect("an inspection");
    assert!(
        matches!(&info.playback, Playback::Broken(reason) if reason.contains("size table")),
        "{:?}",
        info.playback
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* --------------------------------------- matroska tail checks */

/// One EBML element with a one- or two-byte size.
fn ebml(id: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    let size = body.len();
    if size < 0x7F {
        out.push(0x80 | size as u8);
    } else {
        out.push(0x40 | (size >> 8) as u8);
        out.push(size as u8);
    }
    out.extend_from_slice(body);
    out
}

/// The same element, declaring a size its body does not have — which is
/// exactly what a download cut short leaves behind.
fn overrunning(id: &[u8], body: &[u8], declared: usize) -> Vec<u8> {
    let mut out = id.to_vec();
    out.push(0x40 | (declared >> 8) as u8);
    out.push(declared as u8);
    out.extend_from_slice(body);
    out
}

fn webm_clip(declared_cluster: Option<usize>) -> Vec<u8> {
    let header = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"webm"));
    let mut info = ebml(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40]);
    info.extend(ebml(&[0x44, 0x89], &1000.0f64.to_be_bytes()));
    let mut entry = ebml(&[0xD7], &[1]);
    entry.extend(ebml(&[0x83], &[1]));
    entry.extend(ebml(&[0x86], b"V_VP9"));
    let mut payload = ebml(&[0xE7], &[0]);
    payload.extend(ebml(&[0xA3], &[0x81, 0, 0, 0x80, 1, 2, 3]));

    let mut out = header;
    out.extend([0x18, 0x53, 0x80, 0x67]);
    out.extend([0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    out.extend(ebml(&[0x15, 0x49, 0xA9, 0x66], &info));
    out.extend(ebml(&[0x16, 0x54, 0xAE, 0x6B], &ebml(&[0xAE], &entry)));
    out.extend(match declared_cluster {
        Some(size) => overrunning(&[0x1F, 0x43, 0xB6, 0x75], &payload, size),
        None => ebml(&[0x1F, 0x43, 0xB6, 0x75], &payload),
    });
    out
}

#[test]
fn a_webm_whose_last_cluster_overruns_the_file_is_refused() {
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("webm-short");
    let body = webm_clip(None);
    // A cluster that says it holds twenty bytes more than it does.
    let path = place(&dir, "film.webm", &webm_clip(Some(body.len() - 60 + 20)));

    let info = inspect(&path).expect("an inspection");
    assert!(
        matches!(&info.playback, Playback::Broken(reason) if reason.contains("ends inside")),
        "{:?}",
        info.playback
    );
    let error = check_finished(
        &path,
        Expect {
            extension: "webm",
            video: true,
            audio: false,
            audio_selected: false,
        },
    )
    .expect_err("a truncated download");
    assert!(error.contains("ends inside"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_webm_that_walks_cleanly_is_verified() {
    let _guard = super::super::tools::test_guard();
    let dir = temp_dir("webm-whole");
    let path = place(&dir, "film.webm", &webm_clip(None));

    let info = check_finished(
        &path,
        Expect {
            extension: "webm",
            video: true,
            audio: false,
            audio_selected: false,
        },
    )
    .expect("a whole file");
    assert_eq!(info.playback, Playback::Verified);
    assert_eq!(info.duration, Some(1.0));
    assert_eq!(info.video_codec.as_deref(), Some("V_VP9"));
    let _ = std::fs::remove_dir_all(&dir);
}
