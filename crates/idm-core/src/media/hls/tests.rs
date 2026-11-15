    use super::*;
    use crate::media::fixtures;
    use crate::media::{Format, MediaKind};

    const BASE: &str = "https://cdn.example.org/hls/master.m3u8";

    fn audio_names(master: &Master) -> Vec<&str> {
        master
            .audio
            .iter()
            .map(|audio| audio.name.as_str())
            .collect()
    }

    #[test]
    fn a_master_playlist_is_read_with_its_audio_group() {
        let master = parse_master(fixtures::HLS_MASTER, BASE).expect("a master");
        assert_eq!(master.variants.len(), 2);

        let low = &master.variants[0];
        assert_eq!(low.id, "v0");
        assert_eq!(low.width, Some(640));
        assert_eq!(low.height, Some(360));
        assert_eq!(low.bandwidth, Some(800_000));
        assert_eq!(low.frame_rate, Some(30.0));
        assert_eq!(low.audio_group.as_deref(), Some("aud"));
        assert_eq!(
            low.uri, "https://cdn.example.org/hls/low/index.m3u8",
            "a relative address is resolved against the playlist"
        );

        let high = &master.variants[1];
        assert_eq!(high.height, Some(1080));
        assert_eq!(high.frame_rate, Some(60.0));

        assert_eq!(audio_names(&master), vec!["English"]);
        let audio = &master.audio[0];
        assert_eq!(audio.group, "aud");
        assert!(audio.default);
        assert_eq!(audio.uri, "https://cdn.example.org/hls/audio.m3u8");
        assert_eq!(master.audio_group("aud").len(), 1);
    }

    #[test]
    fn a_media_playlist_is_read_with_its_init_segment() {
        let media = parse_media(fixtures::HLS_MEDIA, BASE).expect("a media playlist");
        assert_eq!(media.segments.len(), 2);
        assert!(media.ended);
        assert_eq!(media.duration, 7.5);
        assert_eq!(media.target_duration, Some(4.0));
        assert_eq!(media.version, Some(7));

        let init = media.init.expect("an init segment");
        assert_eq!(init.url, "https://cdn.example.org/hls/init.mp4");
        assert_eq!(init.duration, 0.0);

        assert_eq!(
            media.segments[0].url,
            "https://cdn.example.org/hls/seg0.m4s"
        );
        assert_eq!(media.segments[0].duration, 4.0);
        assert_eq!(media.segments[1].duration, 3.5);
    }

    #[test]
    fn a_byte_range_carries_its_offset_and_a_bare_length_follows_the_previous_one() {
        let text = "#EXTM3U\n\
            #EXT-X-VERSION:4\n\
            #EXT-X-TARGETDURATION:4\n\
            #EXTINF:4,\n\
            #EXT-X-BYTERANGE:100@0\n\
            all.ts\n\
            #EXTINF:4,\n\
            #EXT-X-BYTERANGE:50\n\
            all.ts\n\
            #EXT-X-ENDLIST\n";
        let media = parse_media(text, "https://cdn.example.org/a/index.m3u8").expect("a playlist");
        assert_eq!(media.segments[0].range, Some((0, 99)));
        assert_eq!(media.segments[1].range, Some((100, 149)));
        assert_eq!(media.segments[0].url, media.segments[1].url);
    }

    #[test]
    fn a_relative_address_is_resolved_against_the_real_playlist() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\n../parts/a.ts\n#EXT-X-ENDLIST\n";
        let media =
            parse_media(text, "https://cdn.example.org/live/720/index.m3u8").expect("a playlist");
        assert_eq!(
            media.segments[0].url,
            "https://cdn.example.org/live/parts/a.ts"
        );
    }

    #[test]
    fn an_encrypted_playlist_is_refused_with_one_sentence() {
        let error = parse_media(fixtures::HLS_ENCRYPTED, BASE).expect_err("encrypted");
        assert_eq!(error, MediaError::protected("AES-128"));
        assert!(error.to_string().contains("protected"), "{error}");
        assert!(error.to_string().contains("AES-128"), "{error}");
    }

    #[test]
    fn a_key_names_its_system_and_a_live_playlist_is_not_encryption() {
        let widevine = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n\
            #EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"\n\
            #EXTINF:2,\na.ts\n#EXT-X-ENDLIST\n";
        let error = parse_media(widevine, BASE).expect_err("widevine");
        assert!(error.to_string().contains("Widevine"), "{error}");
        assert!(!error.to_string().contains("live"), "{error}");

        // A session key on a master is the same refusal, named the same way.
        let master = "#EXTM3U\n\
            #EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=800000\nlow/index.m3u8\n";
        let error = parse_master(master, BASE).expect_err("fairplay");
        assert!(error.to_string().contains("FairPlay"), "{error}");

        // A live stream is not encryption, and says so.
        let live = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\na.ts\n";
        let error = parse_media(live, BASE).expect_err("live");
        assert_eq!(error, MediaError::Live);
        assert!(!error.to_string().contains("protected"), "{error}");
    }

    #[test]
    fn a_playlist_that_changes_its_init_segment_is_refused() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI=\"a.mp4\"\n\
            #EXTINF:2,\ns0.m4s\n#EXT-X-MAP:URI=\"b.mp4\"\n\
            #EXTINF:2,\ns1.m4s\n#EXT-X-ENDLIST\n";
        let error = parse_media(text, BASE).expect_err("two maps");
        assert!(matches!(error, MediaError::Unsupported(_)), "{error:?}");
        assert!(error.to_string().contains("init segment"), "{error}");
    }

    #[test]
    fn the_same_init_segment_may_be_named_again() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI=\"a.mp4\"\n\
            #EXTINF:2,\ns0.m4s\n#EXT-X-MAP:URI=\"a.mp4\"\n\
            #EXTINF:2,\ns1.m4s\n#EXT-X-ENDLIST\n";
        let media = parse_media(text, BASE).expect("one map");
        assert_eq!(media.segments.len(), 2);
        assert_eq!(
            media.init.expect("a map").url,
            "https://cdn.example.org/hls/a.mp4"
        );
    }

    #[test]
    fn an_init_segment_range_needs_its_offset() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n\
            #EXT-X-MAP:URI=\"all.mp4\",BYTERANGE=\"100\"\n\
            #EXTINF:2,\n#EXT-X-BYTERANGE:200@0\nall.mp4\n#EXT-X-ENDLIST\n";
        let error = parse_media(text, BASE).expect_err("no offset");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
        assert!(error.to_string().contains("offset"), "{error}");
    }

    #[test]
    fn a_range_without_an_offset_must_continue_the_same_resource() {
        let first = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n\
            #EXTINF:2,\n#EXT-X-BYTERANGE:100\nall.ts\n#EXT-X-ENDLIST\n";
        let error = parse_media(first, BASE).expect_err("no previous range");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
        assert!(error.to_string().contains("offset"), "{error}");

        let other = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n\
            #EXTINF:2,\n#EXT-X-BYTERANGE:100@0\nall.ts\n\
            #EXTINF:2,\n#EXT-X-BYTERANGE:50\nother.ts\n#EXT-X-ENDLIST\n";
        let error = parse_media(other, BASE).expect_err("another resource");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_range_that_runs_past_the_address_space_is_refused() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\n\
            #EXT-X-BYTERANGE:18446744073709551615@18446744073709551615\n\
            all.ts\n#EXT-X-ENDLIST\n";
        let error = parse_media(text, BASE).expect_err("overflow");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_gap_is_refused_rather_than_joined_around() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\na.ts\n\
            #EXT-X-GAP\n#EXTINF:2,\nb.ts\n#EXT-X-ENDLIST\n";
        let error = parse_media(text, BASE).expect_err("a gap");
        assert!(matches!(error, MediaError::Unsupported(_)), "{error:?}");
        assert!(error.to_string().contains("gap"), "{error}");
    }

    #[test]
    fn a_fragmented_playlist_cannot_be_joined_across_a_discontinuity() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI=\"a.mp4\"\n\
            #EXTINF:2,\ns0.m4s\n#EXT-X-DISCONTINUITY\n#EXTINF:2,\ns1.m4s\n#EXT-X-ENDLIST\n";
        let error = parse_media(text, BASE).expect_err("a discontinuity");
        assert!(matches!(error, MediaError::Unsupported(_)), "{error:?}");
        assert!(error.to_string().contains("DISCONTINUITY"), "{error}");
    }

    #[test]
    fn a_plain_playlist_keeps_its_discontinuity_as_a_note() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\na.ts\n\
            #EXT-X-DISCONTINUITY\n#EXTINF:2,\nb.ts\n#EXT-X-ENDLIST\n";
        let media = parse_media(text, BASE).expect("plain segments");
        assert_eq!(media.discontinuities, 1);
        assert_eq!(media.segments.len(), 2);
        assert!(media.init.is_none());
    }

    #[test]
    fn a_key_of_method_none_is_not_encryption() {
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:2,\na.ts\n#EXT-X-ENDLIST\n";
        assert!(parse_media(text, BASE).is_ok());
    }

    #[test]
    fn a_truncated_playlist_is_refused() {
        let error = parse_master(fixtures::HLS_BAD, BASE).expect_err("truncated");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_media_playlist_without_an_end_is_live() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\na.ts\n";
        assert_eq!(parse_media(text, BASE).unwrap_err(), MediaError::Live);
    }

    #[test]
    fn a_playlist_without_the_header_is_refused() {
        assert!(matches!(
            parse_media("#EXTINF:2,\na.ts\n", BASE),
            Err(MediaError::Malformed(_))
        ));
    }

    #[test]
    fn the_kinds_of_a_playlist_are_told_apart() {
        assert!(looks_like_master(fixtures::HLS_MASTER));
        assert!(!looks_like_master(fixtures::HLS_MEDIA));
        assert!(
            parse(fixtures::HLS_MASTER, BASE)
                .expect("a manifest")
                .container_hint
                .as_deref()
                == Some("hls")
        );
    }

    #[test]
    fn a_playlist_over_the_segment_cap_is_refused() {
        let mut text = String::from(
            "#EXTM3U
#EXT-X-TARGETDURATION:1
",
        );
        for _ in 0..=MAX_SEGMENTS {
            text.push_str(
                "#EXTINF:1,
a.ts
",
            );
        }
        text.push_str(
            "#EXT-X-ENDLIST
",
        );
        assert!(matches!(
            parse_media(&text, BASE),
            Err(MediaError::TooLarge(_))
        ));
    }

    #[test]
    fn a_playlist_longer_than_twelve_hours_is_refused() {
        let text = "#EXTM3U
#EXT-X-TARGETDURATION:18000
            #EXTINF:18000,
a.ts
#EXTINF:18000,
b.ts
#EXTINF:18000,
c.ts
#EXT-X-ENDLIST
";
        assert!(matches!(
            parse_media(text, BASE),
            Err(MediaError::TooLarge(_))
        ));
    }

    #[test]
    fn an_attribute_list_keeps_quoted_commas_together() {
        let attrs = Attributes::parse(r#"BANDWIDTH=1,CODECS="avc1.4d401e,mp4a.40.2",AUDIO="aud""#);
        assert_eq!(attrs.get("CODECS"), Some("avc1.4d401e,mp4a.40.2"));
        assert_eq!(attrs.get("AUDIO"), Some("aud"));
        assert_eq!(attrs.get("BANDWIDTH"), Some("1"));
    }

    #[test]
    fn a_master_playlist_offers_its_caption_renditions() {
        let master = parse_master(fixtures::HLS_MASTER_SUBTITLES, BASE).expect("a master");
        assert_eq!(master.subtitles.len(), 2);

        let english = &master.subtitles[0];
        assert_eq!(english.name, "English");
        assert_eq!(english.language.as_deref(), Some("en"));
        assert!(english.is_default);
        assert_eq!(english.url, "https://cdn.example.org/hls/subs/en.m3u8");
        assert_eq!(english.container, "vtt");

        let portuguese = &master.subtitles[1];
        assert_eq!(portuguese.language.as_deref(), Some("pt-BR"));
        assert!(!portuguese.is_default);
    }

    #[test]
    fn a_caption_rendition_becomes_a_row_of_its_own() {
        let parsed = parse_with_subtitles(fixtures::HLS_MASTER_SUBTITLES, BASE).expect("a master");
        let rows: Vec<&Format> = parsed
            .manifest
            .formats
            .iter()
            .filter(|format| format.kind == MediaKind::Subtitle)
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "s0-en-default");
        assert_eq!(rows[0].label, "English (en, default)");
        assert_eq!(rows[0].container, "vtt");
        assert_eq!(rows[1].id, "s1-pt-BR");
        assert_eq!(rows[1].label, "Portuguese (pt-BR)");
    }

    #[test]
    fn a_playlist_without_captions_has_no_caption_rows() {
        let parsed = parse_with_subtitles(fixtures::HLS_MASTER, BASE).expect("a master");
        assert!(parsed.renditions.is_empty());
        assert!(parsed
            .manifest
            .formats
            .iter()
            .all(|format| format.kind != MediaKind::Subtitle));
    }
