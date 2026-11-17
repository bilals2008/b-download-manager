//! The rules every entry passes, whatever the format it came from:
//! - a path is relative, has no `..`, no drive or device prefix and no `:`
//!   (an NTFS stream name would let `a.txt:Zone.Identifier` overwrite the
//!   file's mark), and each part is made a safe file name; it cannot leave
//!   the staging folder;
//! - links, devices and pipes are left out, and nothing is written through a
//!   link;
//! - limits on what is written (a bomb), on the number of entries and on how
//!   deep the folders go;
//! - what was written goes into an empty staging folder, and is moved into
//!   place afterwards without overwriting anything.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::subfolder::Subfolder;
use crate::{Error, Report};

/// What an unpacking may write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Bytes in all.
    pub total: u64,
    /// Entries in all.
    pub entries: u64,
    /// Folders inside folders.
    pub depth: usize,
}

impl Limits {
    /// For an archive of `size` bytes: at most 200 times its size or 20 GiB,
    /// whichever is more (real archives compress well, bombs a thousandfold),
    /// 100 000 entries and 64 levels.
    pub fn for_archive(size: u64) -> Limits {
        Limits {
            total: size.saturating_mul(200).max(20 << 30),
            entries: 100_000,
            depth: 64,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits::for_archive(0)
    }
}

/// Why an entry's name may not be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// It starts at the root of a drive.
    AbsolutePath,
    /// It climbs out of the folder with `..`.
    OutsideFolder,
    /// It carries a drive (`C:`), a device or a stream name.
    DriveOrStream,
    /// It is deeper than the limits allow.
    TooDeep,
    /// Nothing is left of it.
    Empty,
    /// It is not a plain path.
    NotPlain,
    /// It is a link, and a link can point anywhere.
    Link,
}

impl Refusal {
    /// The words the report carries for this refusal.
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::AbsolutePath => "absolute path",
            Refusal::OutsideFolder => "goes outside the folder",
            Refusal::DriveOrStream => "drive or stream name",
            Refusal::TooDeep => "folders too deep",
            Refusal::Empty => "empty name",
            Refusal::NotPlain => "not a plain path",
            Refusal::Link => "link",
        }
    }
}

/// The check every entry's name goes through.
#[derive(Clone, Copy, Debug)]
pub struct Guard {
    depth: usize,
}

impl Guard {
    /// A guard that lets folders be `depth` levels deep.
    pub fn new(depth: usize) -> Guard {
        Guard { depth }
    }

    /// The path an entry's name may be written to, or why it may not.
    ///
    /// Both `/` and `\` separate (Windows tools write `\`).
    pub fn check(&self, name: &str) -> Result<PathBuf, Refusal> {
        if name.starts_with('/') || name.starts_with('\\') {
            return Err(Refusal::AbsolutePath);
        }
        let mut out = PathBuf::new();
        let mut parts = 0usize;
        for part in name.split(['/', '\\']) {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                return Err(Refusal::OutsideFolder);
            }
            // A drive (`C:`), a device or a stream name: never part of a file
            // name here.
            if part.contains(':') {
                return Err(Refusal::DriveOrStream);
            }
            let Some(safe) = sanitize(part) else {
                continue;
            };
            out.push(safe);
            parts += 1;
            if parts > self.depth {
                return Err(Refusal::TooDeep);
            }
        }
        if parts == 0 {
            return Err(Refusal::Empty);
        }
        // Belt and braces: nothing but plain names went in.
        if !out.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(Refusal::NotPlain);
        }
        Ok(out)
    }
}

/// The reserved device names of Windows, which are not files even with an
/// extension after them.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The longest name Windows accepts for one part, with a little room.
const MAX_NAME: usize = 180;

/// One part of a path as a file name Windows accepts, or nothing when
/// nothing is left of it.
fn sanitize(part: &str) -> Option<String> {
    let mut cleaned: String = part
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            other => other,
        })
        .collect();

    cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();

    if cleaned.chars().count() > MAX_NAME {
        cleaned = cleaned.chars().take(MAX_NAME).collect();
    }

    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }

    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        cleaned = format!("_{cleaned}");
    }

    Some(cleaned)
}

/// Whether a folder on the way from `root` to `path` (or `path` itself) is a
/// link: nothing is written through one (a link made by an archive, or by
/// anyone).
fn through_link(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return true;
    };
    let mut at = root.to_path_buf();
    for component in rel.components() {
        at.push(component);
        match fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    false
}

/// Writes entries into the staging folder, counting what is written against
/// the limits.
pub struct Writer<'a> {
    root: PathBuf,
    guard: Guard,
    limits: Limits,
    cancel: Option<&'a AtomicBool>,
    written: u64,
    entries: u64,
    skipped: Vec<(String, &'static str)>,
    refused: u64,
}

impl<'a> Writer<'a> {
    /// A writer that puts what it writes into `root`.
    pub fn new(root: &Path, limits: Limits, cancel: Option<&'a AtomicBool>) -> Self {
        Writer {
            root: root.to_path_buf(),
            guard: Guard::new(limits.depth),
            limits,
            cancel,
            written: 0,
            entries: 0,
            skipped: Vec::new(),
            refused: 0,
        }
    }

    /// How many bytes were written.
    pub fn bytes(&self) -> u64 {
        self.written
    }

    /// The entries left out, with why.
    pub fn take_skipped(&mut self) -> Vec<(String, &'static str)> {
        std::mem::take(&mut self.skipped)
    }

    /// How many entries the guard refused.
    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// The folder the entries go into.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// Left out, with why (kept for the download's log).
    pub fn skip(&mut self, name: &str, why: &'static str) {
        self.skipped.push((name.to_owned(), why));
    }

    /// Refused by the guard: the name would not stay inside the folder.
    fn refuse(&mut self, name: &str, why: Refusal) {
        self.refused += 1;
        self.skipped.push((name.to_owned(), why.reason()));
    }

    fn count(&mut self) -> Result<(), Error> {
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err(Error::TooMany);
        }
        if self.cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(())
    }

    /// A folder entry.
    pub fn dir(&mut self, name: &str) -> Result<(), Error> {
        self.count()?;
        let relative = match self.guard.check(name) {
            Ok(relative) => relative,
            Err(why) => {
                self.refuse(name, why);
                return Ok(());
            }
        };
        let path = self.root.join(relative);
        if through_link(&self.root, &path) {
            self.skip(name, "through a link");
            return Ok(());
        }
        fs::create_dir_all(&path)?;
        Ok(())
    }

    /// A file entry: its bytes read from `data` (at most `size` and a little
    /// more when the archive says how big it is).
    pub fn file(
        &mut self,
        name: &str,
        size: Option<u64>,
        data: &mut dyn Read,
        mode: Option<u32>,
    ) -> Result<(), Error> {
        self.count()?;
        let relative = match self.guard.check(name) {
            Ok(relative) => relative,
            Err(why) => {
                self.refuse(name, why);
                // Read past it: a stream format's next entry follows.
                io::copy(data, &mut io::sink())?;
                return Ok(());
            }
        };
        let mut path = self.root.join(&relative);
        if let Some(parent) = path.parent() {
            if through_link(&self.root, parent) {
                self.skip(name, "through a link");
                io::copy(data, &mut io::sink())?;
                return Ok(());
            }
            fs::create_dir_all(parent)?;
        }
        // Two entries of one name (an archive can hold that): the second is
        // numbered.
        if fs::symlink_metadata(&path).is_ok() {
            path = numbered(&path);
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let entry_cap = size.map(|size| size.saturating_add(1 << 20));
        let mut buffer = vec![0u8; 256 * 1024];
        let mut this = 0u64;
        loop {
            let read = match data.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(read_error(e)),
            };
            this += read as u64;
            self.written += read as u64;
            if entry_cap.is_some_and(|cap| this > cap) {
                return Err(Error::Damaged(format!("{name} is bigger than it says")));
            }
            if self.written > self.limits.total {
                return Err(Error::TooBig {
                    would: self.written,
                    limit: self.limits.total,
                });
            }
            out.write_all(&buffer[..read])?;
            if self.cancelled() {
                return Err(Error::Cancelled);
            }
        }
        out.flush()?;
        drop(out);
        set_mode(&path, mode);
        Ok(())
    }
}

/// A decoder's read error: a wrong password often shows only as data that
/// does not check out.
fn read_error(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::InvalidData {
        Error::Damaged(e.to_string())
    } else {
        Error::Io(e)
    }
}

/// Unix: only the execute bits are kept (never setuid, setgid or sticky).
/// Elsewhere nothing.
#[cfg(unix)]
fn set_mode(path: &Path, mode: Option<u32>) {
    use std::os::unix::fs::PermissionsExt;
    if let Some(mode) = mode {
        let keep = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(keep));
    }
}

/// Windows has no permission bits for an archive to carry.
#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: Option<u32>) {}

/// `name (1).ext`, `name (2).ext`… the first free one beside `path`.
pub fn numbered(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let file = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, extension) = match file.rfind('.') {
        Some(index) if index > 0 => (&file[..index], &file[index..]),
        _ => (file.as_str(), ""),
    };
    (1..)
        .map(|number| parent.join(format!("{stem} ({number}){extension}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| parent.join(format!("{stem} (many){extension}")))
}

/// An empty folder beside the destination that the archive is extracted into;
/// removed with whatever is left in it when dropped (after a failure,
/// everything).
pub struct Staging {
    dest: PathBuf,
    path: PathBuf,
}

static NEXT: AtomicU64 = AtomicU64::new(0);

impl Staging {
    /// An empty staging folder inside `dest`.
    pub fn new(dest: &Path) -> Result<Staging, Error> {
        fs::create_dir_all(dest)?;
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = dest.join(format!(".idm-extract-{}-{number}", std::process::id()));
        if fs::symlink_metadata(&path).is_ok() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir(&path)?;
        Ok(Staging {
            dest: dest.to_path_buf(),
            path,
        })
    }

    /// The folder the entries are written into.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Move what was extracted into place, as `subfolder` says; every file put
    /// there is told to `on_file`. Returns the report's folder and the files
    /// placed.
    pub fn place(
        self,
        base: &str,
        subfolder: Subfolder,
        on_file: &mut dyn FnMut(&Path),
    ) -> Result<(Report, u64), Error> {
        let tops: Vec<PathBuf> = fs::read_dir(&self.path)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        let together = subfolder.wants_folder(tops.len());
        let mut report = Report::default();
        let mut placed = 0;
        if together {
            let name = sanitize(base).unwrap_or_else(|| "extracted".to_owned());
            let mut folder = self.dest.join(&name);
            if fs::symlink_metadata(&folder).is_ok() {
                folder = numbered(&folder);
                report.renamed += 1;
            }
            fs::rename(&self.path, &folder)?;
            placed += mark_all(&folder, on_file)?;
            report.folder = folder;
        } else {
            for top in tops {
                let Some(name) = top.file_name() else {
                    continue;
                };
                let mut to = self.dest.join(name);
                if fs::symlink_metadata(&to).is_ok() {
                    to = numbered(&to);
                    report.renamed += 1;
                }
                fs::rename(&top, &to)?;
                placed += if to.is_dir() {
                    mark_all(&to, on_file)?
                } else {
                    on_file(&to);
                    1
                };
            }
            report.folder = self.dest.clone();
        }
        Ok((report, placed))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Every file under `dir`, told to `on_file`; links are never followed.
/// Iterative (no recursion limit to hit on a deep tree).
fn mark_all(dir: &Path, on_file: &mut dyn FnMut(&Path)) -> io::Result<u64> {
    let mut count = 0u64;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(current) = todo.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                todo.push(entry.path());
            } else {
                on_file(&entry.path());
                count += 1;
            }
        }
    }
    Ok(count)
}

/// An external tool's output in the staging folder, checked as our own
/// entries are: anything that is a link, or not a plain file or folder, is
/// removed before anything is moved into place, and so is a name that is not
/// safe. Returns what was left out.
pub fn check_tree(root: &Path, depth: usize) -> io::Result<Vec<(String, &'static str)>> {
    let guard = Guard::new(depth);
    let mut out = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(current) = todo.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            let kind = fs::symlink_metadata(&path)?.file_type();
            let relative = path
                .strip_prefix(root)
                .map(|relative| relative.to_string_lossy().into_owned())
                .unwrap_or_default();
            let bad = if kind.is_symlink() {
                Some(Refusal::Link.reason())
            } else if !kind.is_dir() && !kind.is_file() {
                Some("not a file")
            } else {
                match guard.check(&relative) {
                    Ok(safe) if safe.to_string_lossy() == relative => None,
                    Ok(_) => Some("unsafe name"),
                    Err(why) => Some(why.reason()),
                }
            };
            match bad {
                Some(why) => {
                    out.push((relative, why));
                    if kind.is_dir() && !kind.is_symlink() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path).or_else(|_| fs::remove_dir(&path));
                    }
                }
                None if kind.is_dir() => todo.push(path),
                None => {}
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_paths_stay_inside() {
        let guard = Guard::new(64);
        assert_eq!(
            guard.check("a/b.txt").unwrap(),
            Path::new("a").join("b.txt")
        );
        assert_eq!(
            guard.check("a\\b.txt").unwrap(),
            Path::new("a").join("b.txt")
        );
        assert_eq!(guard.check("./a//b").unwrap(), Path::new("a").join("b"));
        for bad in [
            "../evil",
            "a/../../b",
            "..\\evil",
            "/abs",
            "\\abs",
            "C:\\x",
            "C:x",
            "x:ads",
            "a.txt:Zone.Identifier",
            "",
            "./",
        ] {
            assert!(guard.check(bad).is_err(), "{bad} should be refused");
        }
        // Device names and trailing dots become safe names, not refusals.
        assert_ne!(guard.check("CON.txt").unwrap(), Path::new("CON.txt"));
        assert!(Guard::new(64).check(&"a/".repeat(65)).is_err());
    }

    #[test]
    fn a_refusal_says_why() {
        assert_eq!(Refusal::OutsideFolder.reason(), "goes outside the folder");
        assert_eq!(Refusal::AbsolutePath.reason(), "absolute path");
        assert_eq!(Refusal::Link.reason(), "link");
    }

    #[test]
    fn a_second_file_of_one_name_gets_a_number() {
        let folder = std::env::temp_dir().join("idm-archive-numbering-test");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");

        let first = folder.join("film.mp4");
        std::fs::write(&first, b"x").expect("a file");
        let second = numbered(&first);
        assert_eq!(second.file_name().unwrap(), "film (1).mp4");
        std::fs::write(&second, b"y").expect("a file");
        assert_eq!(numbered(&first).file_name().unwrap(), "film (2).mp4");

        let _ = std::fs::remove_dir_all(&folder);
    }
}
