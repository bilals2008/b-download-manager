    use super::*;
    use crate::media::fixtures;
    use crate::media::Format;

    const BASE: &str = "https://cdn.example.org/dash/manifest.mpd";

    fn ids(manifest: &DashManifest) -> Vec<&str> {
        manifest
            .tracks
            .iter()
            .map(|track| track.format.id.as_str())
            .collect()
    }

    #[test]
    fn the_fixture_manifest_is_read_with_both_tracks() {
        let parsed = parse_full(fixtures::DASH_MANIFEST, BASE).expect("an MPD");
        assert_eq!(parsed.duration, Some(10.0));
        assert_eq!(ids(&parsed), vec!["1080", "720", "251", "140"]);

        let video = &parsed.tracks[0];
        assert_eq!(video.format.kind, MediaKind::Video);
        assert_eq!(video.format.height, Some(1080));
        assert_eq!(
            video.format.fps,
            Some(30.0),
            "frameRate is inherited from the set"
        );
        assert_eq!(
            video.segments.len(),
            3,
            "ten seconds of four-second segments"
        );
        assert_eq!(
            video.segments[0].url,
            "https://cdn.example.org/dash/v-1080-1.m4s"
        );
        assert_eq!(
            video.init.as_ref().map(|init| init.url.as_str()),
            Some("https://cdn.example.org/dash/v-1080-init.mp4")
        );

        let manifest = &parsed.manifest;
        assert_eq!(manifest.formats.len(), 4);
        assert_eq!(manifest.pairs.len(), 2);
        assert_eq!(manifest.pairs[0].video, "1080");
        assert!(manifest.pairs[0].audio.is_some());
        assert_eq!(
            manifest.formats.last().map(|format| format.kind),
            Some(MediaKind::Audio)
        );
    }

    #[test]
    fn every_template_placeholder_is_expanded() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT8S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" duration="4000" startNumber="7"
                media="v-$RepresentationID$-$Bandwidth$-$Number%05d$-$Time$.m4s"
                initialization="init-$RepresentationID$.mp4"/>
              <Representation id="137" bandwidth="4200000" width="1920" height="1080"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a template");
        let track = &parsed.tracks[0];
        assert_eq!(
            track.segments[0].url,
            "https://cdn.example.org/dash/v-137-4200000-00007-0.m4s"
        );
        assert_eq!(
            track.segments[1].url,
            "https://cdn.example.org/dash/v-137-4200000-00008-4000.m4s"
        );
        assert_eq!(track.segments.len(), 2);
    }

    #[test]
    fn a_segment_timeline_is_expanded() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" media="s-$Time$-$Number$.m4s" startNumber="1">
                <SegmentTimeline><S t="0" d="2000" r="1"/><S d="2000"/></SegmentTimeline>
              </SegmentTemplate>
              <Representation id="v" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a timeline");
        let urls: Vec<&str> = parsed.tracks[0]
            .segments
            .iter()
            .map(|segment| segment.url.as_str())
            .collect();
        assert_eq!(urls.len(), 3);
        assert!(urls[0].ends_with("s-0-1.m4s"));
        assert!(urls[1].ends_with("s-2000-2.m4s"));
        assert!(urls[2].ends_with("s-4000-3.m4s"));
    }

    #[test]
    fn a_template_that_would_expand_past_the_cap_is_refused() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6000S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate timescale="1" duration="1" media="s-$Number$.m4s"/>
            <Representation id="v" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        assert!(matches!(
            parse_full(text, BASE),
            Err(MediaError::TooLarge(_))
        ));
    }

    #[test]
    fn a_segment_list_is_read() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="audio" mimeType="audio/mp4">
            <Representation id="a" bandwidth="128000">
              <SegmentList>
                <Initialization sourceURL="init.mp4" range="0-99"/>
                <SegmentURL media="a1.m4s"/>
                <SegmentURL media="a2.m4s"/>
              </SegmentList>
            </Representation>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a list");
        let track = &parsed.tracks[0];
        assert_eq!(
            track.init.as_ref().map(|init| init.range),
            Some(Some((0, 99)))
        );
        assert_eq!(track.segments.len(), 2);
        assert!(track.segments[0].url.ends_with("a1.m4s"));
    }

    #[test]
    fn a_segment_base_is_one_file() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="137" bandwidth="1000" width="640" height="360">
              <BaseURL>whole.mp4</BaseURL>
              <SegmentBase indexRange="100-200"/>
            </Representation>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a base");
        let track = &parsed.tracks[0];
        assert_eq!(track.segments.len(), 1);
        assert_eq!(
            track.segments[0].url,
            "https://cdn.example.org/dash/whole.mp4"
        );
    }

    #[test]
    fn youtube_itag_ids_survive() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT2S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate media="v.m4s" initialization="i.mp4" timescale="1000" duration="1000"/>
            <Representation id="299" bandwidth="9000" width="1920" height="1080"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("an itag id");
        assert_eq!(parsed.tracks[0].format.id, "299");
    }

    #[test]
    fn a_protected_manifest_is_refused() {
        let text = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011"/>
          <Representation id="1"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(text, BASE).expect_err("protected");
        assert_eq!(
            error,
            MediaError::protected("MPEG Common Encryption"),
            "the protection system is named"
        );
        assert!(error.to_string().contains("protected"));
    }

    #[test]
    fn a_protection_system_is_named_instead_of_one_blanket_sentence() {
        let text = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <ContentProtection
            schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed"/>
          <Representation id="1"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(text, BASE).expect_err("widevine");
        assert!(error.to_string().contains("Widevine"), "{error}");

        // A `ContentProtection` on the representation is read the same way.
        let per_rep = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <Representation id="1"><BaseURL>a.mp4</BaseURL>
            <ContentProtection schemeIdUri="urn:uuid:9a04f079-9840-4286-ab92-e65be0885f95"/>
          </Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(per_rep, BASE).expect_err("playready");
        assert!(error.to_string().contains("PlayReady"), "{error}");
    }

    #[test]
    fn a_dynamic_manifest_is_live() {
        let text = r#"<MPD type="dynamic"><Period><AdaptationSet contentType="video">
          <Representation id="1"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(text, BASE).expect_err("live");
        assert_eq!(error, MediaError::Live);
        assert!(error.to_string().contains("live"));
        assert!(!error.to_string().contains("protected"));
    }

    #[test]
    fn segment_information_is_inherited_from_the_period() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period>
            <SegmentTemplate timescale="1000" duration="2000" media="v-$Number$.m4s"/>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <Representation id="137" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a period-level template");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].segments.len(), 2);
        assert!(parsed.tracks[0].segments[0].url.ends_with("v-1.m4s"));
    }

    #[test]
    fn the_first_period_stops_where_the_next_one_starts() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT10S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" duration="2000" media="v-$Number$.m4s"/>
              <Representation id="1" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
          <Period start="PT4S">
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <Representation id="2"><BaseURL>second.mp4</BaseURL></Representation>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("the first period");
        assert_eq!(
            parsed.tracks[0].segments.len(),
            2,
            "four seconds of two-second segments, not the whole ten-second presentation"
        );
        assert_eq!(
            parsed.duration,
            Some(4.0),
            "what the panel promises is what the first period holds"
        );
    }

    #[test]
    fn a_repeat_that_runs_to_the_period_end_stops_there() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" media="s-$Time$.m4s">
                <SegmentTimeline><S t="0" d="2000" r="-1"/></SegmentTimeline>
              </SegmentTemplate>
              <Representation id="1" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a repeat");
        assert_eq!(
            parsed.tracks[0].segments.len(),
            3,
            "six seconds of two-second segments"
        );
        assert!(parsed.tracks[0].segments[2].url.ends_with("s-4000.m4s"));
    }

    #[test]
    fn a_presentation_time_offset_shifts_the_first_segment_time() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate timescale="1000" duration="2000" presentationTimeOffset="9000"
              media="s-$Number$-$Time$.m4s"/>
            <Representation id="1" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("an offset");
        let urls: Vec<&str> = parsed.tracks[0]
            .segments
            .iter()
            .map(|segment| segment.url.as_str())
            .collect();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].ends_with("s-1-9000.m4s"), "{}", urls[0]);
        assert!(urls[1].ends_with("s-2-11000.m4s"), "{}", urls[1]);
    }

    #[test]
    fn a_timeline_entry_without_a_duration_is_refused() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate timescale="1000" media="s-$Time$.m4s">
              <SegmentTimeline><S t="0" d="0"/></SegmentTimeline>
            </SegmentTemplate>
            <Representation id="1" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let error = parse_full(text, BASE).expect_err("a zero duration");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
        assert!(error.to_string().contains("duration"), "{error}");
    }

    #[test]
    fn a_track_with_no_address_says_that_instead_of_disappearing() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="1" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let error = parse_full(text, BASE).expect_err("no address");
        assert!(matches!(error, MediaError::Unsupported(_)), "{error:?}");
        assert!(error.to_string().contains("no address"), "{error}");
    }

    #[test]
    fn a_second_period_is_noted_and_the_first_is_offered() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="1" bandwidth="1000" width="640" height="360">
              <BaseURL>first.mp4</BaseURL><SegmentBase/>
            </Representation>
          </AdaptationSet></Period>
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="2"><BaseURL>second.mp4</BaseURL><SegmentBase/></Representation>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("the first period");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].format.id, "1");
        assert!(parsed
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("more than one period")));
    }

    #[test]
    fn bad_xml_is_refused() {
        assert!(matches!(
            parse_full(fixtures::DASH_BAD, BASE),
            Err(MediaError::Malformed(_))
        ));
    }

    #[test]
    fn a_duration_is_read_in_seconds() {
        assert_eq!(parse_duration("PT10S"), Some(10.0));
        assert_eq!(parse_duration("PT1H2M3.5S"), Some(3723.5));
        assert_eq!(parse_duration("P1DT2H"), Some(93_600.0));
        assert_eq!(parse_duration("P1M"), None, "a month has no honest seconds");
        assert_eq!(parse_duration("nonsense"), None);
    }

    #[tokio::test]
    async fn the_fixture_tracks_download_and_concatenate() {
        use crate::limits::Limits;
        use crate::media::download::{self, TrackOutcome, TrackSource};
        use crate::media::fixtures;
        use crate::media::testserver::TestServer;

        let video_init = fixtures::segment("VINIT");
        let video_1 = fixtures::segment("V1");
        let video_2 = fixtures::segment("V2");
        let video_3 = fixtures::segment("V3");
        let audio_init = fixtures::segment("AINIT");
        let audio_1 = fixtures::segment("A1");
        let audio_2 = fixtures::segment("A2");
        let audio_3 = fixtures::segment("A3");

        let server = TestServer::start_with_files(vec![
            ("/dash/v-1080-init.mp4".to_string(), video_init.clone()),
            ("/dash/v-1080-1.m4s".to_string(), video_1.clone()),
            ("/dash/v-1080-2.m4s".to_string(), video_2.clone()),
            ("/dash/v-1080-3.m4s".to_string(), video_3.clone()),
            ("/dash/a-251-init.mp4".to_string(), audio_init.clone()),
            ("/dash/a-251-1.m4s".to_string(), audio_1.clone()),
            ("/dash/a-251-2.m4s".to_string(), audio_2.clone()),
            ("/dash/a-251-3.m4s".to_string(), audio_3.clone()),
        ]);
        let parsed = parse_full(fixtures::DASH_MANIFEST, &server.url("/dash/manifest.mpd"))
            .expect("the fixture MPD");

        let dir = std::env::temp_dir().join(format!("idm-dash-download-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");

        let video = parsed
            .tracks
            .iter()
            .find(|track| track.format.id == "1080")
            .expect("the 1080 track");
        let outcome = download::download_track(
            Limits::default(),
            TrackSource {
                init: video.init.clone(),
                segments: video.segments.clone(),
                dest: dir.join("video.mp4"),
                work_dir: dir.clone(),
                stem: "video".to_string(),
                connections: 2,
                proxy: None,
                referer: None,
                headers: Vec::new(),
                header_origin: None,
                fingerprint: None,
            },
            None,
            None,
        )
        .await
        .expect("the video track");
        assert_eq!(outcome, TrackOutcome::Done);

        let mut expected = video_init;
        expected.extend(video_1);
        expected.extend(video_2);
        expected.extend(video_3);
        assert_eq!(
            std::fs::read(dir.join("video.mp4")).expect("the video"),
            expected
        );

        let audio = parsed
            .tracks
            .iter()
            .find(|track| track.format.id == "251")
            .expect("the audio track");
        let outcome = download::download_track(
            Limits::default(),
            TrackSource {
                init: audio.init.clone(),
                segments: audio.segments.clone(),
                dest: dir.join("audio.m4a"),
                work_dir: dir.clone(),
                stem: "audio".to_string(),
                connections: 2,
                proxy: None,
                referer: None,
                headers: Vec::new(),
                header_origin: None,
                fingerprint: None,
            },
            None,
            None,
        )
        .await
        .expect("the audio track");
        assert_eq!(outcome, TrackOutcome::Done);

        let mut expected = audio_init;
        expected.extend(audio_1);
        expected.extend(audio_2);
        expected.extend(audio_3);
        assert_eq!(
            std::fs::read(dir.join("audio.m4a")).expect("the audio"),
            expected
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_frame_rate_is_read_as_a_ratio() {
        assert_eq!(number_f64("30"), Some(30.0));
        assert_eq!(number_f64("30000/1001"), Some(29.97002997002997));
        assert_eq!(number_f64("x"), None);
    }

    #[test]
    fn a_manifest_offers_its_caption_renditions() {
        let parsed = parse_with_subtitles(fixtures::DASH_SUBTITLES, BASE).expect("an MPD");
        assert_eq!(parsed.renditions.len(), 2);

        let english = &parsed.renditions[0];
        assert_eq!(english.name, "sub-en");
        assert_eq!(english.language.as_deref(), Some("en"));
        assert!(english.is_default);
        assert_eq!(english.url, "https://cdn.example.org/dash/subs/en.vtt");
        assert_eq!(english.container, "vtt");

        let portuguese = &parsed.renditions[1];
        assert_eq!(portuguese.language.as_deref(), Some("pt-BR"));
        assert!(!portuguese.is_default);
    }

    #[test]
    fn a_caption_rendition_becomes_a_row_of_its_own() {
        let parsed = parse_with_subtitles(fixtures::DASH_SUBTITLES, BASE).expect("an MPD");
        let rows: Vec<&Format> = parsed
            .manifest
            .formats
            .iter()
            .filter(|format| format.kind == MediaKind::Subtitle)
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "s0-en-default");
        assert_eq!(rows[0].label, "sub-en (en, default)");
        assert_eq!(rows[1].id, "s1-pt-BR");
    }

    #[test]
    fn a_protected_manifest_is_refused_and_a_clear_one_is_not() {
        let error = parse_full(fixtures::DASH_PROTECTED, BASE).expect_err("protected");
        assert_eq!(error, MediaError::protected("MPEG Common Encryption"));
        assert!(parse_full(fixtures::DASH_MANIFEST, BASE).is_ok());
    }
