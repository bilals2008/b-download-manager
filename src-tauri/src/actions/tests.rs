use super::control::delete_file_of;
use super::destination::rewrite_destination;
use super::queue::{dequeue_row, queue_row};
    use super::*;
    use idm_core::segmenter::{part_path, Segment, SegmentStatus};
    use std::path::Path;
    use std::sync::atomic::Ordering;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("my-idm-clone-actions-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn row(id: u64, dest: &Path) -> Arc<Download> {
        Download::new(
            id,
            "https://example.org/film.mp4".to_string(),
            dest.to_path_buf(),
            4,
            None,
            None,
        )
        .expect("a download")
    }

    #[tokio::test]
    async fn a_finished_file_moves_with_its_metadata() {
        let dir = temp_dir("completed");
        let source = dir.join("old").join("film.mp4");
        std::fs::create_dir_all(source.parent().expect("a folder")).expect("folder");
        std::fs::write(&source, b"done").expect("write");

        let state = AppState::default();
        let download = row(1, &source);
        download.set_meta(Some("Video".to_string()), Some("a note".to_string()));
        download.set_status(Status::Completed);
        state.insert(download);

        let target = dir.join("new").join("film.mp4");
        let written = rewrite_destination(&state, 1, &target, Choice::Ask)
            .await
            .expect("a free name");
        assert_eq!(written, target);
        assert_eq!(std::fs::read(&target).expect("the moved file"), b"done");
        assert!(!source.exists(), "the old file is gone");

        let info = state.get(1).expect("the row").info();
        assert_eq!(info.status, Status::Completed);
        assert_eq!(info.dest, target.to_string_lossy());
        assert_eq!(info.category.as_deref(), Some("Video"));
        assert_eq!(info.description.as_deref(), Some("a note"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_same_file_is_not_a_move() {
        let dir = temp_dir("same");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"done").expect("write");

        let state = AppState::default();
        let download = row(1, &dest);
        download.set_status(Status::Completed);
        state.insert(download);

        let written = rewrite_destination(&state, 1, &dest, Choice::Ask)
            .await
            .expect("its own file is no conflict");
        assert_eq!(written, dest);
        assert_eq!(std::fs::read(&dest).expect("the file"), b"done");
        assert_eq!(state.get(1).expect("the row").dest, dest);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_taken_name_asks_before_a_finished_file_moves() {
        let dir = temp_dir("taken");
        let source = dir.join("film.mp4");
        std::fs::write(&source, b"new").expect("write");
        let taken = dir.join("taken").join("film.mp4");
        std::fs::create_dir_all(taken.parent().expect("a folder")).expect("folder");
        std::fs::write(&taken, b"old").expect("write");

        let state = AppState::default();
        let download = row(1, &source);
        download.set_status(Status::Completed);
        state.insert(download);

        let refusal = rewrite_destination(&state, 1, &taken, Choice::Ask)
            .await
            .expect_err("a question");
        assert_eq!(refusal.kind, "exists");
        assert_eq!(
            std::fs::read(&taken).expect("the file that was there"),
            b"old"
        );

        let moved = rewrite_destination(&state, 1, &taken, Choice::Rename)
            .await
            .expect("a free name");
        assert_eq!(moved, dir.join("taken").join("film (2).mp4"));
        assert_eq!(
            std::fs::read(&taken).expect("the file that was there"),
            b"old"
        );
        assert_eq!(std::fs::read(&moved).expect("the moved file"), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_replaced_file_only_goes_after_the_answer() {
        let dir = temp_dir("replace");
        let source = dir.join("film.mp4");
        std::fs::write(&source, b"new").expect("write");
        let taken = dir.join("taken").join("film.mp4");
        std::fs::create_dir_all(taken.parent().expect("a folder")).expect("folder");
        std::fs::write(&taken, b"old").expect("write");

        let state = AppState::default();
        let download = row(1, &source);
        download.set_status(Status::Completed);
        state.insert(download);

        let written = rewrite_destination(&state, 1, &taken, Choice::Replace)
            .await
            .expect("replace");
        assert_eq!(written, taken);
        assert_eq!(std::fs::read(&taken).expect("the replaced file"), b"new");
        assert!(!source.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_unfinished_download_takes_its_parts_along() {
        let dir = temp_dir("parts");
        let source = dir.join("film.mp4");
        let state = AppState::default();
        let download = row(1, &source);
        download.set_status(Status::Paused);
        let part = part_path(&download.dir, &download.stem, 0);
        std::fs::write(&part, b"half").expect("write");
        {
            let mut segments = download.segments.lock().expect("segments");
            segments.push(Segment {
                id: 0,
                start: 0,
                end: 10,
                downloaded: 4,
                part: part.clone(),
                status: SegmentStatus::Paused,
            });
        }
        download.total.store(10, Ordering::SeqCst);
        state.insert(download);

        let target = dir.join("new").join("film.mp4");
        rewrite_destination(&state, 1, &target, Choice::Ask)
            .await
            .expect("a free name");

        let info = state.get(1).expect("the row").info();
        assert_eq!(info.status, Status::Paused, "a move is not a resume");
        assert_eq!(info.downloaded, 4, "the bytes travel with the row");
        let new_part = part_path(target.parent().expect("a folder"), "film.mp4", 0);
        assert_eq!(std::fs::read(&new_part).expect("the moved part"), b"half");
        assert!(!part.exists());
        assert!(
            target.with_file_name("film.mp4.idm.json").exists(),
            "the manifest goes with the parts"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_row_with_a_task_cannot_change_its_file() {
        let dir = temp_dir("busy");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Running);
        state.insert(download);

        let refusal = rewrite_destination(&state, 1, &dir.join("other.mp4"), Choice::Ask)
            .await
            .expect_err("a running row");
        assert_eq!(refusal.kind, "busy");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_finished_file_removes_the_file_and_the_row() {
        let dir = temp_dir("delete");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"done").expect("write");

        let state = AppState::default();
        let download = row(1, &dest);
        download.set_status(Status::Completed);
        state.insert(download);

        delete_file_of(&state, 1).expect("the file is deleted");
        assert!(!dest.exists());
        assert!(state.get(1).is_none(), "the row is gone too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_not_deleted_silently() {
        let dir = temp_dir("missing");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Completed);
        state.insert(download);

        let error = delete_file_of(&state, 1).expect_err("nothing to delete");
        assert!(error.contains("not there"), "the error was: {error}");
        assert!(state.get(1).is_some(), "the record stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_running_download_keeps_its_file() {
        let dir = temp_dir("delete-busy");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"part").expect("write");

        let state = AppState::default();
        let download = row(1, &dest);
        download.set_status(Status::Running);
        state.insert(download);

        let error = delete_file_of(&state, 1).expect_err("a running row");
        assert!(
            error.contains("stop the download"),
            "the error was: {error}"
        );
        assert!(dest.exists(), "the file is left alone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_queue_parks_a_row_and_lets_it_out_again() {
        let dir = temp_dir("queue");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Paused);
        state.insert(download);

        queue_row(&state, 1).expect("the row is queued");
        assert_eq!(state.get(1).expect("the row").status(), Status::Queued);
        // A second click is not a failure.
        queue_row(&state, 1).expect("already queued");

        dequeue_row(&state, 1).expect("the row leaves the queue");
        assert_eq!(state.get(1).expect("the row").status(), Status::Paused);

        state.get(1).expect("the row").set_status(Status::Completed);
        assert!(
            queue_row(&state, 1).is_err(),
            "a finished row has nothing to queue"
        );
        assert!(dequeue_row(&state, 1).is_err(), "it is not in the queue");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_category_folder_decides_where_a_start_lands() {
        let dir = temp_dir("category-folder");
        let films = dir.join("Films");
        let state = AppState::default();
        state.set_categories(vec![
            crate::state::CategoryRule {
                name: "Video".to_string(),
                extensions: vec!["mp4".to_string(), "mkv".to_string()],
                folder: String::new(),
                builtin: true,
                fallback: false,
            },
            crate::state::CategoryRule {
                name: "Films".to_string(),
                extensions: vec!["mp4".to_string()],
                folder: films.to_string_lossy().to_string(),
                builtin: false,
                fallback: false,
            },
            crate::state::CategoryRule {
                name: "Relative".to_string(),
                extensions: vec!["zip".to_string()],
                folder: "parts\\nowhere".to_string(),
                builtin: false,
                fallback: false,
            },
        ]);

        // No category was named, so the file type decides — and a rule the
        // reader wrote wins over the built-in one claiming the same type.
        let (dest, category) = category_destination(&state, dir.join("film.mp4"), None, true);
        assert_eq!(category.as_deref(), Some("Films"));
        assert_eq!(dest, films.join("film.mp4"));

        // The add dialog clears the flag when the reader picked a folder.
        let (dest, _) = category_destination(
            &state,
            dir.join("elsewhere").join("film.mp4"),
            Some("Films".to_string()),
            false,
        );
        assert_eq!(dest, dir.join("elsewhere").join("film.mp4"));

        // A category with no folder, or only a relative one, changes nothing.
        let (dest, category) = category_destination(&state, dir.join("clip.mkv"), None, true);
        assert_eq!(category.as_deref(), Some("Video"));
        assert_eq!(dest, dir.join("clip.mkv"));
        let (dest, _) = category_destination(&state, dir.join("pack.zip"), None, true);
        assert_eq!(dest, dir.join("pack.zip"), "a relative folder is not used");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_queue_position_can_be_changed() {
        let dir = temp_dir("queue-pos");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Queued);
        state.insert(download);
        state.queue().enqueue(1);
        state.queue().enqueue(2);

        set_queue_position(&state, 2, 0).expect("move to front");
        let order = state.queue().snapshot().order;
        assert_eq!(order, vec![2, 1]);

        set_queue_position(&state, 1, 1).expect("move to second");
        let order = state.queue().snapshot().order;
        assert_eq!(order, vec![2, 1]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_queue_position_out_of_range_is_refused() {
        let dir = temp_dir("queue-pos-bad");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Queued);
        state.insert(download);
        state.queue().enqueue(1);

        assert!(
            set_queue_position(&state, 1, 5).is_err(),
            "past the end of the queue"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_queue_position_for_unknown_download_is_refused() {
        let state = AppState::default();
        assert!(
            set_queue_position(&state, 99, 0).is_err(),
            "not in the queue"
        );
    }

    #[test]
    fn the_max_simultaneous_can_be_changed() {
        let state = AppState::default();
        assert_eq!(state.queue().max_simultaneous(), 3);
        let rate = set_max_simultaneous(&state, 7);
        assert_eq!(rate, 7);
        assert_eq!(state.queue().max_simultaneous(), 7);
        let rate = set_max_simultaneous(&state, 0);
        assert_eq!(rate, 1, "never below one");
    }
