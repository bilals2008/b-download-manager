//! Where the bytes go: the part files in the folder the application handed
//! over. The crate never decides where a download ends up, it only writes
//! what it was given a place to write.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::error::TorrentError;
use crate::metainfo::Metainfo;
use crate::peerconn::{BlockSource, BoxFuture};
use crate::pieces::PieceMap;
use crate::safe::safe_path;

/// One file of the torrent, being written.
struct StoredFile {
    /// Which file of the torrent this is.
    index: usize,
    /// Where the file goes when it is whole.
    path: PathBuf,
    /// Where the bytes are written while it is not.
    part: PathBuf,
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
            let handle = tokio::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&part)
                .await?;
            handle.set_len(file.length).await?;
            files.push(StoredFile {
                index: *index,
                path,
                part,
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
    pub async fn finish_file(&self, torrent_index: usize) -> Result<PathBuf, TorrentError> {
        let file = self
            .files
            .iter()
            .find(|file| file.index == torrent_index)
            .ok_or_else(|| TorrentError::Failed(format!("file {torrent_index} is not written here")))?;
        tokio::fs::rename(&file.part, &file.path).await?;
        Ok(file.path.clone())
    }

    /// Where a file is written, and where its part file is.
    pub fn paths(&self, torrent_index: usize) -> Option<(PathBuf, PathBuf)> {
        self.files
            .iter()
            .find(|file| file.index == torrent_index)
            .map(|file| (file.path.clone(), file.part.clone()))
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

    fn a_metainfo() -> Metainfo {
        Metainfo {
            info_hash: "0".repeat(40),
            name: "Example".into(),
            piece_length: 4,
            pieces: vec![],
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
}
