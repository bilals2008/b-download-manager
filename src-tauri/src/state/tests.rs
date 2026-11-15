use crate::video::SourcePlan;
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("my-idm-clone-state-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn abandoned_temporaries_are_cleared_and_a_known_stem_is_kept() {
        let dir = temp_dir("clean");
        // The flat layout of the earlier releases: loose files in the old
        // unfinished folder beside the application data.
        let legacy = dir.join("unfinished");
        let final_folder = dir.join("final");
        std::fs::create_dir_all(&legacy).expect("folder");
        std::fs::create_dir_all(&final_folder).expect("folder");

        let state = AppState::default();
        let download = Download::new_in(
            1,
            "https://example.org/kept.mp4".to_string(),
            final_folder.join("kept.mp4"),
            Some(&legacy),
            4,
            None,
            None,
        )
        .expect("a download");
        state.insert(download);

        for name in [
            "kept.mp4.part0",
            "kept.mp4.idm.json",
            "gone.mp4.part0",
            "gone.mp4.idm.json.tmp",
            "gone.mp4.idmtmp",
            "notes.txt",
        ] {
            std::fs::write(legacy.join(name), b"x").expect("write");
        }

        let staging = crate::staging::Staging::resolve(&dir).expect("a staging tree");
        state.set_staging(staging);

        assert!(legacy.join("kept.mp4.part0").exists());
        assert!(legacy.join("kept.mp4.idm.json").exists());
        assert!(!legacy.join("gone.mp4.part0").exists());
        assert!(!legacy.join("gone.mp4.idm.json.tmp").exists());
        assert!(!legacy.join("gone.mp4.idmtmp").exists());
        assert!(
            legacy.join("notes.txt").exists(),
            "a file the engine did not write stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ready_job(id: u64, manifest_url: &str) -> MediaJob {
        use crate::video::{MediaKindHint, MediaState};
        use idm_core::media::{Format, Manifest, MediaKind};

        let mut job = MediaJob::new(
            id,
            "https://www.example.org/watch?v=1".to_string(),
            manifest_url.to_string(),
            MediaKindHint::File,
            "Film".to_string(),
            "https://www.example.org/watch?v=1".to_string(),
            Vec::new(),
            None,
        );
        job.state = MediaState::Ready;
        job.manifest = Some(Manifest {
            title: "Film".to_string(),
            duration: None,
            formats: vec![Format::new("137", MediaKind::Video, "mp4")],
            pairs: Vec::new(),
            container_hint: Some("file".to_string()),
            subtitles: Vec::new(),
        });
        job
    }

    #[test]
    fn a_merged_job_keeps_answering_for_the_id_the_extension_watches() {
        let state = AppState::default();
        state.insert_media_job(ready_job(1, "https://cdn.example.org/a.mp4"));
        state.insert_media_job(ready_job(2, "https://cdn.example.org/b.mp4"));

        state.alias_media_job(2, 1);

        assert_eq!(
            state.media_job(2).map(|job| job.id),
            Some(1),
            "the old id resolves to the job that now holds the tracks"
        );
        state.update_media_job(2, |job| job.title = "Changed".to_string());
        assert_eq!(
            state.media_job(1).map(|job| job.title),
            Some("Changed".to_string()),
            "a write through the old id lands on the merged job"
        );
        assert!(
            state.media_jobs_snapshot().iter().all(|job| job.id != 2),
            "the retired id is not a second row"
        );

        state.remove_media_job(2);
        assert!(
            state.media_job(1).is_none(),
            "forget removes the merged job"
        );
        assert!(state.media_job(2).is_none(), "and every alias with it");
    }

    #[test]
    fn a_manifest_that_is_already_open_reuses_its_job() {
        let state = AppState::default();
        state.insert_media_job(ready_job(4, "https://cdn.example.org/a.mp4"));

        assert_eq!(
            state.media_job_for_manifest("https://cdn.example.org/a.mp4"),
            Some(4)
        );
        assert_eq!(
            state.media_job_for_manifest("https://cdn.example.org/other.mp4"),
            None
        );

        state.update_media_job(4, |job| job.done = true);
        assert_eq!(
            state.media_job_for_manifest("https://cdn.example.org/a.mp4"),
            None,
            "a finished job is not a list to open again"
        );
    }

    #[test]
    fn file_candidates_from_one_page_find_their_target() {
        use crate::video::{FileTrack, MediaJob, MediaKindHint, MediaState};
        use idm_core::media::{Format, Manifest, MediaKind};

        let state = AppState::default();
        let mut job = MediaJob::new(
            1,
            "https://www.example.org/watch?v=1".to_string(),
            "https://cdn.example.org/videoplayback?itag=137".to_string(),
            MediaKindHint::File,
            "Film".to_string(),
            "https://www.example.org/watch?v=1".to_string(),
            Vec::new(),
            None,
        );
        job.state = MediaState::Ready;
        job.manifest = Some(Manifest {
            title: "Film".to_string(),
            duration: None,
            formats: vec![Format::new("137", MediaKind::Video, "mp4")],
            pairs: Vec::new(),
            container_hint: Some("file".to_string()),
            subtitles: Vec::new(),
        });
        job.source = Some(SourcePlan::Files {
            tracks: vec![FileTrack {
                format: Format::new("137", MediaKind::Video, "mp4"),
                lease: crate::video::lease::RequestLease::new(
                    "https://cdn.example.org/videoplayback?itag=137",
                    crate::video::lease::Authority::Native,
                    "iOS",
                    None,
                    None,
                    crate::video::lease::Credentials::at("https://cdn.example.org"),
                    crate::video::lease::Transport::Progressive,
                    crate::video::lease::RepresentationFingerprint {
                        id: "137".to_string(),
                        container: "mp4".to_string(),
                        codec: None,
                        bytes: None,
                    },
                    None,
                ),
            }],
        });
        state.insert_media_job(job);

        assert_eq!(
            state.merge_file_target(2, "https://www.example.org/watch?v=1"),
            Some(1)
        );
        assert_eq!(
            state.merge_file_target(2, "https://www.example.org/watch?v=2"),
            None,
            "a different video is a different job"
        );
        assert_eq!(
            state.merge_file_target(1, "https://www.example.org/watch?v=1"),
            None
        );

        state.update_media_job(1, |job| job.downloading = true);
        assert_eq!(
            state.merge_file_target(2, "https://www.example.org/watch?v=1"),
            None,
            "a running download takes no new tracks"
        );
    }

    #[test]
    fn a_rule_finds_the_category_and_its_folder() {
        let dir = temp_dir("rules");
        let state = AppState::default();
        state.set_categories(vec![
            CategoryRule {
                name: "Video".to_string(),
                extensions: vec!["mp4".to_string(), "mkv".to_string()],
                folder: "relative\\folder".to_string(),
                builtin: true,
                fallback: false,
            },
            CategoryRule {
                name: "Films".to_string(),
                extensions: vec!["mp4".to_string()],
                folder: dir.join("films").to_string_lossy().to_string(),
                builtin: false,
                fallback: false,
            },
            CategoryRule {
                name: "Other".to_string(),
                extensions: Vec::new(),
                folder: dir.join("misc").to_string_lossy().to_string(),
                builtin: true,
                fallback: true,
            },
        ]);

        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\Film.MP4"))
                .as_deref(),
            Some("Films"),
            "a rule the reader wrote wins over a built-in one claiming the same type"
        );
        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\Clip.MKV"))
                .as_deref(),
            Some("Video"),
            "a type only the built-in knows still lands there"
        );
        assert_eq!(
            state.category_folder("Films"),
            Some(dir.join("films")),
            "the folder the rule names"
        );
        assert_eq!(
            state.category_folder("Video"),
            None,
            "a relative folder is not applied"
        );
        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\unknown.bin"))
                .as_deref(),
            Some("Other"),
            "a type nobody claims lands where the window sends it"
        );
        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\README"))
                .as_deref(),
            Some("Other"),
            "a name with no type at all is the same case"
        );
        assert_eq!(state.category_folder("Other"), Some(dir.join("misc")));

        // Without a fallback the answer is no category at all, and the
        // destination stays where the start put it.
        state.set_categories(vec![CategoryRule {
            name: "Video".to_string(),
            extensions: vec!["mp4".to_string()],
            folder: String::new(),
            builtin: true,
            fallback: false,
        }]);
        assert_eq!(
            state.detect_category(Path::new("C:\\Downloads\\unknown.bin")),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
