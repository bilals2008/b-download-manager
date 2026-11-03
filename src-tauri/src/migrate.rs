//! Moving the resume state of older releases into the managed staging tree.
//!
//! The first start of this build finds unfinished downloads in three places:
//! beside the destination file (the oldest layout), loose in the old flat
//! `unfinished` folder, or in a folder a previous release was configured to
//! use. All of them walk into one layout: one folder per download inside
//! `staging/downloads/<id>`. The move never destroys anything it cannot
//! prove it copied: each part reaches its new folder and is measured there
//! before the manifest that records it is rewritten, and the rewritten
//! manifest lands before the old one goes. A download whose move fails is
//! left exactly where it was, still resumable from its old folder; only a
//! verified move changes the index.

use std::path::{Path, PathBuf};

use idm_core::downloader;
use idm_core::log;
use idm_core::session::{self, Index};

use crate::staging::Staging;

/// What one run of the migration did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Downloads whose resume state now lives in the staging tree.
    pub moved: usize,
    /// Downloads left where they are because the move failed.
    pub failed: usize,
}

/// Move every resumable download into the staging tree, then rewrite the
/// index to match.
///
/// Called from setup before the downloads of an earlier run are restored, so
/// no row exists yet and nothing can write over what is being moved. A
/// finished download — the file is at its destination — has nothing to move;
/// its leftovers were removed when it completed, and a manifest that
/// survived an older version is taken along if one is still there.
pub fn run(index: &Index, staging: &Staging) -> Report {
    let mut known = index.load();
    let mut report = Report::default();

    for position in 0..known.len() {
        let dest = PathBuf::from(&known[position].dest);
        let current = known[position]
            .unfinished_dir
            .as_deref()
            .filter(|dir| !dir.trim().is_empty())
            .map(PathBuf::from);
        let manifest = session::manifest_path(&dest, current.as_deref());
        if !manifest.is_file() {
            continue;
        }
        // A finished download has its file at the destination: the manifest
        // beside it is the leftover of a version that kept one, and the
        // parts that go with it are bytes nothing will ever join. They go,
        // and the row stays what it is.
        if dest.is_file() {
            remove_finished_state(&manifest, &dest);
            continue;
        }
        let target = staging.download_dir(&dest, known[position].id);
        if manifest.parent() == Some(target.as_path()) {
            continue;
        }
        match downloader::relocate_resume_state(&manifest, &target) {
            Ok(moved) => {
                known[position].unfinished_dir = Some(target.to_string_lossy().to_string());
                report.moved += 1;
                // The index is written before the old files go: a crash in
                // between leaves the state complete in the new folder, and
                // the index already points at it. The old copy is only
                // removed once the index names the new one.
                match index.save(&known) {
                    Ok(()) => downloader::discard_relocated(&moved),
                    Err(error) => log::write(
                        "warn",
                        &format!(
                            "the index of the migrated downloads could not be written: {error}"
                        ),
                    ),
                }
                log::write(
                    "info",
                    &format!(
                        "moved the resume state of {} into {}",
                        manifest.display(),
                        target.display()
                    ),
                );
            }
            Err(error) => {
                report.failed += 1;
                log::write(
                    "warn",
                    &format!(
                        "the resume state of {} stayed where it is: {error}",
                        manifest.display()
                    ),
                );
            }
        }
    }
    report
}

/// Remove the resume state of a download that already has its file.
///
/// Only the names the engine writes are touched: the manifest, the write
/// temporary of the manifest, and the ranges of this file name.
fn remove_finished_state(manifest: &Path, dest: &Path) {
    let Some(folder) = manifest.parent() else {
        return;
    };
    let stem = manifest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .and_then(|name| name.strip_suffix(".idm.json").map(str::to_string))
        .unwrap_or_default();
    if !stem.is_empty() {
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let ours = name == format!("{stem}.idm.json")
                    || name == format!("{stem}.idm.json.tmp")
                    || name == format!("{stem}.idmtmp")
                    || name
                        .strip_prefix(&format!("{stem}.part"))
                        .is_some_and(|id| {
                            !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
                        });
                if ours {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
    let _ = std::fs::remove_file(manifest);
    log::write(
        "info",
        &format!(
            "the resume state of the finished download {} was cleared",
            dest.display()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use idm_core::downloader::Status;
    use idm_core::session::KnownDownload;
    use std::path::Path;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bdm-migrate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn write_legacy_state(dest_dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dest_dir).expect("folder");
        let manifest = dest_dir.join(format!("{name}.idm.json"));
        let part = dest_dir.join(format!("{name}.part0"));
        std::fs::write(&part, b"12345678").expect("part");
        let body = serde_json::json!({
            "url": "https://example.org/redacted",
            "total": 16,
            "segments": [{
                "id": 0u32, "start": 0u64, "end": 16u64, "downloaded": 8u64,
                "part": part.to_string_lossy(), "status": "pending"
            }],
            "validator": serde_json::Value::Null,
        });
        std::fs::write(&manifest, body.to_string()).expect("manifest");
        manifest
    }

    fn entry(id: u64, dest: &Path, unfinished: Option<&Path>) -> KnownDownload {
        KnownDownload {
            id,
            url: "https://example.org/film.mp4".to_string(),
            dest: dest.to_string_lossy().to_string(),
            connections: 4,
            total: 16,
            downloaded: 8,
            status: Status::Paused,
            error: None,
            referer: None,
            speed_limit: 0,
            category: None,
            description: None,
            replace: false,
            unfinished_dir: unfinished.map(|p| p.to_string_lossy().to_string()),
        }
    }

    #[test]
    fn a_side_by_side_legacy_download_walks_into_the_tree_and_stays_resumable() {
        let dir = temp_dir("beside");
        let downloads = dir.join("Downloads");
        let manifest = write_legacy_state(&downloads, "film.mp4");
        let dest = downloads.join("film.mp4");

        let data = dir.join("appdata");
        std::fs::create_dir_all(&data).expect("data dir");
        let staging = Staging::resolve(&data).expect("a staging tree");
        let index = Index::new(data.join("downloads.json"));
        index
            .save(&[entry(7, &dest, None)])
            .expect("an index to migrate");

        let report = run(&index, &staging);
        assert_eq!(
            report,
            Report {
                moved: 1,
                failed: 0
            }
        );

        let target = staging.download_dir(&dest, 7);
        assert!(
            !manifest.exists(),
            "the old manifest goes last, and it went"
        );
        assert!(!downloads.join("film.mp4.part0").exists());
        assert!(target.join("film.mp4.idm.json").is_file());
        assert_eq!(
            std::fs::read(target.join("film.mp4.part0")).expect("the moved part"),
            b"12345678"
        );

        // The rewritten manifest records the new paths, and the engine reads
        // it back for the row.
        let known = index.load();
        assert_eq!(
            known[0].unfinished_dir.as_deref(),
            Some(target.to_string_lossy().as_ref())
        );
        let download =
            idm_core::downloader::Download::from_known(&known[0]).expect("a restored row");
        assert!(download.manifest.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_move_that_cannot_be_verified_leaves_the_old_state_alone() {
        let dir = temp_dir("broken");
        let downloads = dir.join("Downloads");
        std::fs::create_dir_all(&downloads).expect("folder");
        let manifest = downloads.join("film.mp4.idm.json");
        std::fs::write(&manifest, "{ not json").expect("a broken manifest");
        let dest = downloads.join("film.mp4");

        let data = dir.join("appdata");
        std::fs::create_dir_all(&data).expect("data dir");
        let staging = Staging::resolve(&data).expect("a staging tree");
        let index = Index::new(data.join("downloads.json"));
        index
            .save(&[entry(8, &dest, None)])
            .expect("an index to migrate");

        let report = run(&index, &staging);
        assert_eq!(report.moved, 0, "a broken manifest is not moved");
        assert_eq!(report.failed, 1);
        assert!(manifest.exists(), "the resume state was not deleted");

        let known = index.load();
        assert_eq!(known[0].unfinished_dir, None, "the index still says so");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_part_recorded_but_missing_refuses_the_move() {
        let dir = temp_dir("missing-part");
        let downloads = dir.join("Downloads");
        let manifest = write_legacy_state(&downloads, "film.mp4");
        std::fs::remove_file(downloads.join("film.mp4.part0")).expect("drop the part");
        let dest = downloads.join("film.mp4");

        let data = dir.join("appdata");
        std::fs::create_dir_all(&data).expect("data dir");
        let staging = Staging::resolve(&data).expect("a staging tree");
        let index = Index::new(data.join("downloads.json"));
        index
            .save(&[entry(9, &dest, None)])
            .expect("an index to migrate");

        let report = run(&index, &staging);
        assert_eq!(report.moved, 0);
        assert!(manifest.exists(), "nothing is destroyed on a refusal");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_leftover_state_of_a_finished_download_goes() {
        let dir = temp_dir("finished");
        let downloads = dir.join("Downloads");
        std::fs::create_dir_all(&downloads).expect("folder");
        let dest = downloads.join("film.mp4");
        std::fs::write(&dest, b"the finished file").expect("the file");
        let manifest = write_legacy_state(&downloads, "film.mp4");

        let data = dir.join("appdata");
        std::fs::create_dir_all(&data).expect("data dir");
        let staging = Staging::resolve(&data).expect("a staging tree");
        let index = Index::new(data.join("downloads.json"));
        index
            .save(&[entry(11, &dest, Some(&downloads))])
            .expect("an index to migrate");

        let report = run(&index, &staging);
        assert_eq!(report.moved, 0, "a finished download is not moved");
        assert!(
            !manifest.exists(),
            "the resume state of a finished download is cleared"
        );
        assert!(dest.is_file(), "the finished file stays where it is");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_already_in_the_tree_is_not_touched_twice() {
        let dir = temp_dir("already");
        let data = dir.join("appdata");
        std::fs::create_dir_all(&data).expect("data dir");
        let staging = Staging::resolve(&data).expect("a staging tree");
        let downloads = dir.join("Downloads");
        let dest = downloads.join("film.mp4");
        let target = staging.download_dir(&dest, 10);
        let manifest = write_legacy_state(&downloads, "film.mp4");
        // The state is where the staging tree wants it already.
        std::fs::create_dir_all(&target).expect("target");
        std::fs::rename(&manifest, target.join("film.mp4.idm.json")).expect("place it");
        let part = downloads.join("film.mp4.part0");
        std::fs::rename(&part, target.join("film.mp4.part0")).expect("place the part");
        // The manifest inside the tree points at the old paths; a re-run must
        // not fail on a file whose parent already is the target.
        let body = format!(
            "{{\"url\":\"u\",\"total\":16,\"segments\":[{{\"id\":0,\"start\":0,\"end\":16,\
             \"downloaded\":8,\"part\":\"{}\",\"status\":\"pending\"}}],\"validator\":null}}",
            target
                .join("film.mp4.part0")
                .to_string_lossy()
                .replace('\\', "\\\\")
        );
        std::fs::write(target.join("film.mp4.idm.json"), body).expect("rewrite");

        let dest = downloads.join("film.mp4");
        let index = Index::new(data.join("downloads.json"));
        index
            .save(&[entry(10, &dest, Some(&target))])
            .expect("an index");

        let report = run(&index, &staging);
        assert_eq!(
            report,
            Report {
                moved: 0,
                failed: 0
            },
            "the second run of the migration does nothing"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
