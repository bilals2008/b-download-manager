//! Archive detection and safe extraction for the download engine.
//!
//! The download manager offers to unpack an archive when it finishes. This
//! crate knows what an archive is, lists what is inside it, and unpacks it
//! without letting a single entry escape the folder it goes into.
//!
//! Zip (Deflate, Deflate64 and WinZip AES), tar and its gzip and xz forms and
//! a single gzip or xz file are read here. 7z and RAR are handed to 7-Zip,
//! run as its own program: UnRAR's licence forbids linking it into a GPL
//! program, and a 7z decoder in the application costs about a megabyte for a
//! format most people who use it already have 7-Zip for. The program is
//! found, never bundled ([`find_7zip`]).
//!
//! Everything goes through one guard ([`safe`]): an entry whose name would
//! leave the destination folder is refused and counted, links are never
//! written, and the archive is unpacked into an empty staging folder that is
//! moved into place afterwards, so a failure leaves nothing behind but the
//! archive itself.
//!
//! Portable, no unsafe, no UI and no words: the errors are an enum the
//! application words.

mod external;
mod format;
mod listing;
mod safe;
mod single;
mod subfolder;
mod tar_fmt;
mod unpack;
mod volumes;
mod zip_fmt;

pub use external::find_7zip;
pub use format::Format;
pub use listing::{list, Entry};
pub use safe::{Guard, Limits, Refusal};
pub use subfolder::Subfolder;
pub use unpack::is_archive;
pub use volumes::{volume_key, Volume, VolumeKind};

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// How an archive is unpacked.
pub struct Policy<'a> {
    /// The folder the contents go into.
    pub dest: PathBuf,
    /// Whether a folder named after the archive is made for them.
    pub subfolder: Subfolder,
    /// The password of an encrypted archive, when it has one.
    pub password: Option<String>,
    /// How much may be written (a bomb is an archive that writes more than it
    /// says).
    pub limits: Limits,
    /// 7-Zip's program, for 7z and RAR. None: it is looked for with
    /// [`find_7zip`], and those archives fail with [`Error::NoTool`] when it
    /// is not installed.
    pub tool: Option<PathBuf>,
    /// Told every file put in place, so the application can mark it as
    /// downloaded from the internet, as the archive is.
    pub on_file: &'a mut dyn FnMut(&Path),
    /// Set from elsewhere to stop (the download was deleted, the application
    /// is closing).
    pub cancel: Option<&'a AtomicBool>,
}

/// What an unpacking did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Where the contents went (the destination, or the folder made for them).
    pub folder: PathBuf,
    /// How many files were put in place.
    pub files: u64,
    /// How many bytes were written.
    pub bytes: u64,
    /// The entries left out, with why ("link", "goes outside the folder").
    pub skipped: Vec<(String, &'static str)>,
    /// How many entries were refused because their name would not stay inside
    /// the folder.
    pub refused: u64,
    /// How many files were put in place under another name, one of that name
    /// being there already.
    pub renamed: u64,
}

/// Why an archive was not unpacked.
#[derive(Debug)]
pub enum Error {
    /// It is encrypted and no password was given.
    NeedsPassword,
    /// The password is not the one.
    WrongPassword,
    /// Volume `n` of a split archive is not there (1-based; 0 when the tool
    /// did not say which one).
    MissingVolume(u32),
    /// Damaged, or not what its name says; the decoder's own words, for the
    /// log.
    Damaged(String),
    /// It would write more than the limit (a bomb, or not enough free space).
    TooBig {
        /// How many bytes it would write.
        would: u64,
        /// How many bytes are allowed.
        limit: u64,
    },
    /// More entries, or folders deeper, than allowed.
    TooMany,
    /// A 7z or a RAR, and no 7-Zip was found.
    NoTool,
    /// Not an archive this crate reads.
    Unsupported,
    /// The download was deleted, or the application is closing.
    Cancelled,
    /// Something went wrong on the disk.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NeedsPassword => write!(f, "needs a password"),
            Error::WrongPassword => write!(f, "wrong password"),
            Error::MissingVolume(0) => write!(f, "a volume is missing"),
            Error::MissingVolume(n) => write!(f, "volume {n} is missing"),
            Error::Damaged(s) => write!(f, "damaged: {s}"),
            Error::TooBig { would, limit } => {
                write!(f, "would write {would} bytes (limit {limit})")
            }
            Error::TooMany => write!(f, "too many entries or folders too deep"),
            Error::NoTool => write!(f, "7z and RAR archives need 7-Zip, and none was found"),
            Error::Unsupported => write!(f, "not an archive this crate reads"),
            Error::Cancelled => write!(f, "cancelled"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// The archive's name without its archive extensions (`movie.part1.rar` gives
/// `movie`, `photos.tar.gz` gives `photos`, `x.7z.001` gives `x`): the folder
/// [`Subfolder::Always`] makes.
pub fn base_name(archive: &Path) -> String {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(volume) = volume_key(&name) {
        return volume.base;
    }
    let lower = name.to_lowercase();
    for ext in [
        ".tar.gz", ".tar.xz", ".tgz", ".txz", ".zip", ".7z", ".rar", ".tar", ".gz", ".xz",
    ] {
        if lower.ends_with(ext) && lower.len() > ext.len() {
            return name[..name.len() - ext.len()].to_owned();
        }
    }
    name
}

/// Unpack `archive` (the first volume of a split one) as `policy` says.
pub fn extract(archive: &Path, policy: &mut Policy<'_>) -> Result<Report, Error> {
    unpack::run(archive, policy)
}
