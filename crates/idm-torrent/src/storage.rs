//! Where the bytes go: the part files in the folder the application handed
//! over. The crate never decides where a download ends up, it only writes
//! what it was given a place to write.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha1::Digest;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::error::TorrentError;
use crate::metainfo::Metainfo;
use crate::peerconn::{BlockSource, BoxFuture};
use crate::pieces::PieceMap;
use crate::safe::safe_path;
use crate::util::hex_encode;

/// One file of the torrent, being written.
struct StoredFile {
    /// Which file of the torrent this is.
    index: usize,
    /// Where the file goes when it is whole.
    path: PathBuf,
    /// Where the bytes are written while it is not.
    part: PathBuf,
    /// The file that is written to: the part file, or the file itself when it
    /// is already there from an earlier run.
    working: PathBuf,
    /// How many bytes the file holds.
    length: u64,
    /// Where the file starts in the stream of all the bytes.
    offset: u64,
    /// The open file.
    handle: Mutex<tokio::fs::File>,
}

/// The part files of a job.
pub struct Storage {
    root: PathBuf,
    files: Vec<StoredFile>,
    piece_map: PieceMap,
}

impl Storage {
    /// The part files of the chosen files of a torrent, inside `root`.
    ///
    /// A file that is already there is written to where it is: the bytes of
    /// a job that was stopped are never thrown away. A part file that is
    /// shorter than it should be stays short, so the bytes that are missing
    /// are missing and not zeros.
    pub async fn new(
        root: &Path,
        metainfo: &Metainfo,
        chosen: &[usize],
    ) -> Result<Storage, TorrentError> {
        tokio::fs::create_dir_all(root).await?;
        let piece_map = PieceMap::new(metainfo.piece_length, metainfo.total_length);
        let mut files = Vec::with_capacity(chosen.len());
        for index in chosen {
            let file = &metainfo.files[*index];
            let relative = safe_path(&file.path)
                .ok_or_else(|| TorrentError::Unsafe(format!("{} is not a safe name", file.path)))?;
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let part = part_path(&path);
            let working = if path.exists() { path.clone() } else { part.clone() };
            // No truncate: the bytes of a job that was stopped are kept, and
            // the file is never extended with zeros to look complete.
            let handle = tokio::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&working)
                .await?;
            files.push(StoredFile {
                index: *index,
                path,
                part,
                working,
                length: file.length,
                offset: file.offset,
                handle: Mutex::new(handle),
            });
        }
        Ok(Storage {
            root: root.to_path_buf(),
            files,
            piece_map,
        })
    }

    /// The folder the files are written into.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Write a piece that has been checked against its hash. Everything the
    /// files share lives behind a mutex, so reading and writing can happen
    /// on the same storage at the same time.
    pub async fn write_piece(&self, index: u64, data: &[u8]) -> Result<(), TorrentError> {
        let start = index * self.piece_map.piece_length;
        let end = start + data.len() as u64;
        for file in &self.files {
            let file_start = file.offset;
            let file_end = file.offset + file.length;
            if end <= file_start || start >= file_end {
                continue;
            }
            let from = file_start.max(start);
            let to = file_end.min(end);
            let slice = &data[(from - start) as usize..(to - start) as usize];
            let mut handle = file.handle.lock().await;
            handle.seek(std::io::SeekFrom::Start(from - file.offset)).await?;
            handle.write_all(slice).await?;
            handle.flush().await?;
        }
        Ok(())
    }

    /// The files whose pieces are all there, as the indices of the torrent.
    pub fn finished_files(&self, have: &[bool]) -> Vec<usize> {
        let mut finished = Vec::new();
        for file in &self.files {
            let first = self.piece_map.index_at(file.offset);
            let last = self.piece_map.index_at(file.offset + file.length.saturating_sub(1));
            let whole = (first..=last).all(|piece| {
                have.get(piece as usize).copied().unwrap_or(false)
            });
            if whole {
                finished.push(file.index);
            }
        }
        finished
    }

    /// Put a whole file where it belongs: the part file becomes the file.
    /// A file that is already where it belongs is left alone.
    pub async fn finish_file(&self, torrent_index: usize) -> Result<PathBuf, TorrentError> {
        let file = self
            .files
            .iter()
            .find(|file| file.index == torrent_index)
            .ok_or_else(|| TorrentError::Failed(format!("file {torrent_index} is not written here")))?;
        if file.working != file.part {
            return Ok(file.path.clone());
        }
        tokio::fs::rename(&file.part, &file.path).await?;
        Ok(file.path.clone())
    }

}

/// The part file of a file: the file's name with `.part` behind it.
fn part_path(path: &Path) -> PathBuf {
    let mut part = path.to_path_buf();
    let name = part
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "part".into());
    part.set_file_name(format!("{name}.part"));
    part
}

/// The pieces of a torrent that are already on disk.
///
/// The files are read and hashed against the hashes the torrent carries, so
/// a file that is only half there counts as missing: a piece is only there
/// when every byte of it is there and the hash says so. A piece can lie
/// over two files, so the bytes of a file are read in the order of the
/// stream, and the hash of a piece is fed the bytes in that order too.
pub fn pieces_on_disk(
    root: &Path,
    metainfo: &Metainfo,
    chosen: &[usize],
) -> Result<Vec<bool>, TorrentError> {
    let piece_map = PieceMap::new(metainfo.piece_length, metainfo.total_length);
    let mut hashers: Vec<sha1::Sha1> = (0..piece_map.count).map(|_| sha1::Sha1::new()).collect();
    let mut indices: Vec<usize> = chosen.to_vec();
    indices.sort_unstable();
    indices.dedup();

    for index in indices {
        let Some(file) = metainfo.files.get(index) else {
            continue;
        };
        let Some(relative) = safe_path(&file.path) else {
            continue;
        };
        let path = root.join(relative);
        // A file that is whole is read where it is; a file that is still
        // being written is read from its part file.
        let source = if path.exists() { path } else { part_path(&path) };
        let Ok(mut handle) = std::fs::File::open(&source) else {
            continue;
        };
        let file_end = file.offset + file.length;
        let mut at = 0u64;
        while at < file.length {
            let piece = piece_map.index_at(file.offset + at);
            if piece >= piece_map.count {
                break;
            }
            let piece_end = piece_map.offset_of(piece) + piece_map.length_of(piece);
            let to = file_end.min(piece_end);
            let amount = (to - (file.offset + at)) as usize;
            let mut buffer = vec![0u8; amount];
            // A file that is shorter than it should be stops here: the rest
            // of the piece is missing, and its hash will not match.
            if handle.read_exact(&mut buffer).is_err() {
                break;
            }
            hashers[piece as usize].update(&buffer);
            at += amount as u64;
        }
    }

    let mut have = Vec::with_capacity(piece_map.count as usize);
    for (index, hasher) in hashers.into_iter().enumerate() {
        let mut digest = [0u8; 20];
        digest.copy_from_slice(&hasher.finalize());
        let matches = metainfo
            .pieces
            .get(index)
            .map(|hash| hash.eq_ignore_ascii_case(&hex_encode(&digest)))
            .unwrap_or(false);
        have.push(matches);
    }
    Ok(have)
}

impl BlockSource for Storage {
    fn read_block(
        self: Arc<Self>,
        index: u32,
        begin: u32,
        length: u32,
    ) -> BoxFuture<'static, Option<Vec<u8>>> {
        Box::pin(async move {
            let piece_map = self.piece_map;
            let start = index as u64 * piece_map.piece_length + begin as u64;
            let end = start + length as u64;
            let mut out = Vec::with_capacity(length as usize);
            for file in &self.files {
                let file_start = file.offset;
                let file_end = file.offset + file.length;
                if end <= file_start || start >= file_end {
                    continue;
                }
                let from = file_start.max(start);
                let to = file_end.min(end);
                let mut handle = file.handle.lock().await;
                let mut buffer = vec![0u8; (to - from) as usize];
                if handle.seek(std::io::SeekFrom::Start(from - file.offset)).await.is_err() {
                    return None;
                }
                if handle.read_exact(&mut buffer).await.is_err() {
                    return None;
                }
                out.extend_from_slice(&buffer);
            }
            if out.len() == length as usize {
                Some(out)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metainfo::TorrentFile;
    use crate::util::sha1_bytes;

    /// The bytes the torrent of the tests holds.
    fn content() -> Vec<u8> {
        b"abcdefgh".to_vec()
    }

    fn a_metainfo() -> Metainfo {
        let bytes = content();
        let pieces = [&bytes[..4], &bytes[4..]]
            .iter()
            .map(|chunk| hex_encode(&sha1_bytes(chunk)))
            .collect();
        Metainfo {
            info_hash: "0".repeat(40),
            name: "Example".into(),
            piece_length: 4,
            pieces,
            files: vec![
                TorrentFile {
                    path: "Example/one.txt".into(),
                    length: 6,
                    offset: 0,
                },
                TorrentFile {
                    path: "Example/two.txt".into(),
                    length: 2,
                    offset: 6,
                },
            ],
            trackers: vec![],
            total_length: 8,
        }
    }

    /// A folder with the files of the torrent in it, whole.
    fn a_folder(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("Example")).unwrap();
        std::fs::write(folder.join("Example").join("one.txt"), &bytes[..6]).unwrap();
        std::fs::write(folder.join("Example").join("two.txt"), &bytes[6..]).unwrap();
        folder
    }

    #[tokio::test]
    async fn a_piece_is_written_into_the_files() {
        let folder = std::env::temp_dir().join("idm-torrent-storage-test");
        let _ = tokio::fs::remove_dir_all(&folder).await;
        let storage = Storage::new(&folder, &a_metainfo(), &[0, 1]).await.unwrap();
        storage.write_piece(0, b"abcd").await.unwrap();
        storage.write_piece(1, b"efgh").await.unwrap();
        for index in storage.finished_files(&[true, true]) {
            storage.finish_file(index).await.unwrap();
        }
        let one = tokio::fs::read(folder.join("Example").join("one.txt")).await.unwrap();
        let two = tokio::fs::read(folder.join("Example").join("two.txt")).await.unwrap();
        assert_eq!(one, b"abcdef");
        assert_eq!(two, b"gh");
        let _ = tokio::fs::remove_dir_all(&folder).await;
    }

    #[tokio::test]
    async fn the_bytes_of_a_stopped_job_are_kept() {
        let folder = std::env::temp_dir().join("idm-torrent-storage-keep");
        let _ = tokio::fs::remove_dir_all(&folder).await;
        tokio::fs::create_dir_all(folder.join("Example")).await.unwrap();
        tokio::fs::write(folder.join("Example").join("one.txt.part"), b"abcdef")
            .await
            .unwrap();

        let _storage = Storage::new(&folder, &a_metainfo(), &[0, 1]).await.unwrap();
        // The file is not moved to its whole name, and the part file keeps
        // every byte it had.
        assert!(!folder.join("Example").join("one.txt").exists());
        let part = tokio::fs::read(folder.join("Example").join("one.txt.part"))
            .await
            .unwrap();
        assert_eq!(part, b"abcdef");
        let _ = tokio::fs::remove_dir_all(&folder).await;
    }

    #[test]
    fn a_folder_with_the_right_files_holds_every_piece() {
        let folder = a_folder("idm-torrent-storage-check", &content());
        let have = pieces_on_disk(&folder, &a_metainfo(), &[0, 1]).unwrap();
        assert_eq!(have, vec![true, true]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_folder_with_a_wrong_byte_is_missing_that_piece() {
        let mut bytes = content();
        bytes[1] = b'X';
        let folder = a_folder("idm-torrent-storage-wrong", &bytes);
        let have = pieces_on_disk(&folder, &a_metainfo(), &[0, 1]).unwrap();
        // The first piece holds the byte that is wrong; the second is whole.
        assert_eq!(have, vec![false, true]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_folder_with_a_short_file_is_missing_its_pieces() {
        let folder = std::env::temp_dir().join("idm-torrent-storage-short");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("Example")).unwrap();
        std::fs::write(folder.join("Example").join("one.txt"), b"abc").unwrap();
        let have = pieces_on_disk(&folder, &a_metainfo(), &[0, 1]).unwrap();
        assert_eq!(have, vec![false, false]);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
