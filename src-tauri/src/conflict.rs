//! What to do when the destination file is already there.
//!
//! A download never writes over a file by accident. The add dialog and the
//! browser bridge both come through here, and the answer is settled again
//! while the start is claimed: a file that appeared after the dialog opened,
//! or a second copy of the same download, is a conflict rather than a silent
//! replacement.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::dest;
use crate::state::AppState;

/// The longest name one component of a path may have, matching `dest.rs`.
const MAX_NAME: usize = 200;

/// What the user picked in the File Exists dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// Report the conflict so the window can ask.
    #[default]
    Ask,
    /// Save under a free name beside the existing file.
    Rename,
    /// Replace the existing file, which the user confirmed.
    Replace,
    /// Leave the existing file alone and start nothing.
    Skip,
}

/// The outcome of a start that got past the destination question.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// The download that was created. `None` when the user skipped it.
    pub id: Option<u64>,
    /// The file the download actually writes, which a rename changes.
    pub dest: String,
    /// Whether the existing file was left alone and nothing was created.
    pub skipped: bool,
}

/// A start that the window has to ask about, or cannot perform.
///
/// This crosses the Tauri boundary as an object, so the dialog can show the
/// path and the name it suggests instead of parsing a sentence.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    /// `exists`, `active`, `busy`, `invalid` or `unsafe`.
    pub kind: String,
    /// One sentence in the user's words.
    pub message: String,
    /// The file the start asked for.
    pub path: String,
    /// A free name beside it, ready for the Rename action.
    pub suggested: String,
}

impl Refusal {
    fn new(kind: &str, message: String, path: &Path, suggested: &Path) -> Self {
        Self {
            kind: kind.to_string(),
            message,
            path: path.to_string_lossy().to_string(),
            suggested: suggested.to_string_lossy().to_string(),
        }
    }
}

/// What the add dialog needs to know about a destination.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationStatus {
    pub folder: String,
    pub folder_exists: bool,
    /// A file of that name is already there.
    pub file_exists: bool,
    /// Another row in the list uses that exact file.
    pub active: bool,
    /// A free name beside the file, for the Rename action.
    pub suggested_name: String,
    pub suggested_path: String,
}

/// Look at a destination, including the name a rename would pick.
pub fn inspect(state: &AppState, dest: &Path) -> Result<DestinationStatus, String> {
    let seen = dest::inspect(dest)?;
    let suggested = unique_destination(dest, &state.destinations());
    Ok(DestinationStatus {
        folder: seen.folder,
        folder_exists: seen.folder_exists,
        file_exists: seen.file_exists,
        active: state
            .destinations()
            .iter()
            .any(|active| same_path(active, dest)),
        suggested_name: suggested
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        suggested_path: suggested.to_string_lossy().to_string(),
    })
}

/// A start that has passed the destination question.
pub enum Resolved {
    Start { dest: PathBuf, replace: bool },
    Skip,
}

/// Settle the destination of a start.
///
/// The caller holds the start gate, so a second start cannot slip in between
/// the check and the insert of the download. `except` names the row that is
/// being started again: its own file is not a conflict with itself.
pub fn resolve(
    state: &AppState,
    dest: &Path,
    choice: Choice,
    except: Option<u64>,
) -> Result<Resolved, Refusal> {
    // Skipping creates nothing, so there is nothing about the destination
    // that could be wrong with it — even when the folder has gone away since
    // the window asked.
    if choice == Choice::Skip {
        return Ok(Resolved::Skip);
    }

    let busy = state.destinations_except(except);
    dest::validate(dest).map_err(|message| {
        Refusal::new("invalid", message, dest, &unique_destination(dest, &busy))
    })?;

    let active = busy.iter().any(|active| same_path(active, dest));
    let exists = dest.exists();

    if !active && !exists {
        return Ok(Resolved::Start {
            dest: dest.to_path_buf(),
            replace: false,
        });
    }

    let suggested = unique_destination(dest, &busy);
    match choice {
        Choice::Ask => Err(Refusal::new(
            if active { "active" } else { "exists" },
            if active {
                "Another download is already using this file.".to_string()
            } else {
                "A file with this name is already there.".to_string()
            },
            dest,
            &suggested,
        )),
        Choice::Rename => Ok(Resolved::Start {
            dest: suggested,
            replace: false,
        }),
        Choice::Replace if active => Err(Refusal::new(
            "active",
            "Another download is using this file, so it cannot be replaced. Save this one under a new name."
                .to_string(),
            dest,
            &suggested,
        )),
        Choice::Replace => Ok(Resolved::Start {
            dest: dest.to_path_buf(),
            replace: true,
        }),
        // Settled above, before the destination was looked at.
        Choice::Skip => Ok(Resolved::Skip),
    }
}

/// The first free name beside `dest`: `film (2).mp4`, `film (3).mp4`, ...
pub fn unique_destination(dest: &Path, active: &[PathBuf]) -> PathBuf {
    let folder = dest::folder_of(dest);
    let stem = dest
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "download".to_string());
    let extension = dest
        .extension()
        .map(|extension| extension.to_string_lossy().to_string())
        .filter(|extension| !extension.is_empty());

    for number in 2..=9999 {
        let suffix = format!(" ({number})");
        let room = MAX_NAME.saturating_sub(
            suffix.chars().count() + extension.as_ref().map_or(1, |e| e.chars().count() + 1),
        );
        let short: String = stem.chars().take(room.max(1)).collect();
        let name = match &extension {
            Some(extension) => format!("{short}{suffix}.{extension}"),
            None => format!("{short}{suffix}"),
        };
        let candidate = folder.join(name);
        if !candidate.exists() && !active.iter().any(|other| same_path(other, &candidate)) {
            return candidate;
        }
    }

    // A folder with ten thousand files of the same name is not a normal
    // destination. A time-stamped name is still better than overwriting one.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    folder.join(format!("{stem}-{stamp}"))
}

/// Whether two paths name the same file on this platform.
pub fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "my-idm-clone-conflict-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn test_download(id: u64, dest: &Path) -> std::sync::Arc<idm_core::downloader::Download> {
        idm_core::downloader::Download::new(
            id,
            "https://example.org/film.mp4".to_string(),
            dest.to_path_buf(),
            4,
            None,
            None,
        )
        .expect("a download")
    }

    #[test]
    fn a_free_name_is_offered_beside_a_taken_one() {
        let dir = temp_dir("rename");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"old").expect("write");
        let unique = unique_destination(&dest, &[]);
        assert_eq!(unique, dir.join("film (2).mp4"));

        std::fs::write(&unique, b"taken").expect("write");
        assert_eq!(
            unique_destination(&dest, &[]),
            dir.join("film (3).mp4"),
            "the next free number is used"
        );

        let long = dir.join(format!("{}.zip", "a".repeat(199)));
        let renamed = unique_destination(&long, &[]);
        let name = renamed.file_name().unwrap().to_string_lossy();
        assert!(name.chars().count() <= MAX_NAME, "name was {name}");
        assert!(name.ends_with(".zip"), "the extension is kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_a_running_download_holds_is_not_offered() {
        let dir = temp_dir("active");
        let dest = dir.join("film.mp4");
        let held = dir.join("film (2).mp4");
        let unique = unique_destination(&dest, std::slice::from_ref(&held));
        assert_ne!(unique, held);
        assert_eq!(unique, dir.join("film (3).mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_taken_file_asks_before_anything_starts() {
        let dir = temp_dir("resolve");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"old").expect("write");
        let state = AppState::default();

        let asked = resolve(&state, &dest, Choice::Ask, None)
            .err()
            .expect("a taken name is a question");
        assert_eq!(asked.kind, "exists");
        assert_eq!(asked.path, dest.to_string_lossy());
        assert_eq!(asked.suggested, dir.join("film (2).mp4").to_string_lossy());

        match resolve(&state, &dest, Choice::Rename, None).expect("rename") {
            Resolved::Start { dest, replace } => {
                assert_eq!(dest, dir.join("film (2).mp4"));
                assert!(!replace, "a new name has nothing to replace");
            }
            Resolved::Skip => panic!("rename starts a download"),
        }

        match resolve(&state, &dest, Choice::Replace, None).expect("replace") {
            Resolved::Start {
                dest: kept,
                replace,
            } => {
                assert_eq!(kept, dest, "replace keeps the name the user saw");
                assert!(replace);
            }
            Resolved::Skip => panic!("replace starts a download"),
        }

        assert!(matches!(
            resolve(&state, &dest, Choice::Skip, None).expect("skip"),
            Resolved::Skip
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_row_restarted_from_its_own_file_is_not_a_conflict() {
        let dir = temp_dir("restart");
        let dest = dir.join("film.mp4");
        let state = AppState::default();
        state.insert(test_download(1, &dest));

        let free = resolve(&state, &dest, Choice::Ask, Some(1))
            .expect("its own row does not hold the name");
        assert!(matches!(free, Resolved::Start { .. }));

        // Another row asking for the same file is still a question.
        let asked = resolve(&state, &dest, Choice::Ask, Some(2))
            .err()
            .expect("another row holds the name");
        assert_eq!(asked.kind, "active");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_copy_is_not_a_third_when_the_first_is_finished() {
        let dir = temp_dir("same");
        let a = dir.join("Report.PDF");
        let b = dir.join("report.pdf");
        assert_eq!(same_path(&a, &b), cfg!(windows));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
