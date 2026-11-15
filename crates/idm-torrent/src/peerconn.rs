//! A connection to one peer: the handshake, the messages that go over it and
//! the pieces that come back.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::bencode::{bencode_decode, bencode_encode, BencodeValue};
use crate::error::TorrentError;
use crate::peer::{Handshake, Message, BLOCK_SIZE};

/// A future that is not tied to a lifetime.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where the blocks of a piece come from when they are served to a peer.
pub trait BlockSource: Send + Sync {
    /// The bytes of a block, or nothing when they are not there.
    fn read_block(
        self: Arc<Self>,
        index: u32,
        begin: u32,
        length: u32,
    ) -> BoxFuture<'static, Option<Vec<u8>>>;
}

/// A peer we are talking to.
pub struct PeerSession {
    reader: OwnedReadHalf,
    writer: Mutex<OwnedWriteHalf>,
    /// The address of the peer.
    pub address: SocketAddr,
    /// The id the peer says.
    pub peer_id: [u8; 20],
    piece_count: u64,
    have: Vec<bool>,
    choked: bool,
    interested: bool,
    /// How many bytes came from this peer.
    pub downloaded: u64,
    /// How many bytes went to this peer.
    pub uploaded: u64,
    source: Option<Arc<dyn BlockSource>>,
}

impl PeerSession {
    /// Connect to a peer and say hello.
    pub async fn connect(
        address: SocketAddr,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        piece_count: u64,
    ) -> Result<PeerSession, TorrentError> {
        let stream = tokio::time::timeout(
            Duration::from_secs(15),
            TcpStream::connect(address),
        )
        .await
        .map_err(|_| TorrentError::Network("the peer did not answer".into()))?
        .map_err(|e| TorrentError::Network(e.to_string()))?;
        let (reader, writer) = stream.into_split();

        let mut session = PeerSession {
            reader,
            writer: Mutex::new(writer),
            address,
            peer_id: [0; 20],
            piece_count,
            have: vec![false; piece_count as usize],
            choked: false,
            interested: false,
            downloaded: 0,
            uploaded: 0,
            source: None,
        };

        session
            .send_raw(&Handshake::new(info_hash, peer_id).encode())
            .await?;
        let mut bytes = [0u8; 68];
        tokio::time::timeout(Duration::from_secs(15), session.reader.read_exact(&mut bytes))
            .await
            .map_err(|_| TorrentError::Network("the peer did not say hello".into()))?
            .map_err(|e| TorrentError::Network(e.to_string()))?;
        let handshake = Handshake::decode(&bytes)?;
        if handshake.info_hash != info_hash {
            return Err(TorrentError::Malformed(
                "the peer is on another torrent".into(),
            ));
        }
        session.peer_id = handshake.peer_id;
        Ok(session)
    }

    /// Where the blocks of a piece come from when this peer asks for them.
    pub fn with_source(mut self, source: Arc<dyn BlockSource>) -> Self {
        self.source = Some(source);
        self
    }

    /// Whether the peer says it has a piece.
    pub fn has_piece(&self, index: u32) -> bool {
        self.have.get(index as usize).copied().unwrap_or(false)
    }

    /// How much of the torrent the peer has, from 0 to 1.
    pub fn progress(&self) -> f32 {
        if self.piece_count == 0 {
            return 0.0;
        }
        self.have.iter().filter(|has| **has).count() as f32 / self.piece_count as f32
    }

    /// Send a message.
    pub async fn send(&self, message: &Message) -> Result<(), TorrentError> {
        self.send_raw(&message.to_wire()).await
    }

    async fn send_raw(&self, bytes: &[u8]) -> Result<(), TorrentError> {
        let mut writer = self.writer.lock().await;
        writer
            .write_all(bytes)
            .await
            .map_err(|e| TorrentError::Network(e.to_string()))?;
        writer
            .flush()
            .await
            .map_err(|e| TorrentError::Network(e.to_string()))?;
        Ok(())
    }

    /// Read the next message.
    pub async fn read_message(&mut self) -> Result<Message, TorrentError> {
        let mut length = [0u8; 4];
        tokio::time::timeout(Duration::from_secs(60), self.reader.read_exact(&mut length))
            .await
            .map_err(|_| TorrentError::Network("the peer went quiet".into()))?
            .map_err(|e| TorrentError::Network(e.to_string()))?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 {
            return Ok(Message::KeepAlive);
        }
        if length > 1 << 20 {
            return Err(TorrentError::Malformed(
                "the peer sent a message that is too long".into(),
            ));
        }
        let mut body = vec![0u8; length];
        self.reader
            .read_exact(&mut body)
            .await
            .map_err(|e| TorrentError::Network(e.to_string()))?;
        Message::decode(body[0], &body[1..])
    }

    /// Say which pieces we have.
    pub async fn send_bitfield(&self, have: &[bool]) -> Result<(), TorrentError> {
        let mut bytes = vec![0u8; have.len().div_ceil(8)];
        for (index, has) in have.iter().enumerate() {
            if *has {
                bytes[index / 8] |= 1 << (7 - index % 8);
            }
        }
        self.send(&Message::Bitfield(bytes)).await
    }

    /// Ask for a piece and wait until it is whole.
    pub async fn download_piece(
        &mut self,
        index: u32,
        length: u64,
    ) -> Result<Vec<u8>, TorrentError> {
        let mut data = vec![0u8; length as usize];
        if !self.interested {
            self.send(&Message::Interested).await?;
            self.interested = true;
        }

        let mut begin = 0u64;
        while begin < length {
            let block = (length - begin).min(BLOCK_SIZE as u64) as u32;
            self.send(&Message::Request {
                index,
                begin: begin as u32,
                length: block,
            })
            .await?;
            begin += block as u64;
        }

        let mut received = 0u64;
        while received < length {
            let message = self.read_message().await?;
            match message {
                Message::Piece {
                    index: got,
                    begin: at,
                    data: bytes,
                } => {
                    if got != index {
                        continue;
                    }
                    let start = at as usize;
                    let end = start + bytes.len();
                    if end > data.len() {
                        continue;
                    }
                    data[start..end].copy_from_slice(&bytes);
                    received += bytes.len() as u64;
                    self.downloaded += bytes.len() as u64;
                }
                Message::Choke => {
                    self.choked = true;
                    return Err(TorrentError::Network("the peer choked us".into()));
                }
                Message::Unchoke => self.choked = false,
                Message::Have(piece) => self.set_have(piece),
                Message::Bitfield(bytes) => self.set_bitfield(&bytes),
                Message::Request {
                    index: piece,
                    begin: at,
                    length: block,
                } => self.serve_request(piece, at, block).await?,
                Message::NotInterested | Message::KeepAlive | Message::Interested => {}
                Message::Cancel { .. } | Message::Port(_) | Message::Extended { .. } => {}
            }
        }
        Ok(data)
    }

    /// Ask the peer for the metadata of the torrent (BEP 9).
    pub async fn fetch_metadata(&mut self) -> Result<Vec<u8>, TorrentError> {
        let hello = BencodeValue::Dict(vec![
            (
                "m".into(),
                BencodeValue::Dict(vec![("ut_metadata".into(), BencodeValue::Integer(1))]),
            ),
            ("v".into(), BencodeValue::Bytes(b"idm-torrent".to_vec())),
        ]);
        self.send(&Message::Extended {
            id: 0,
            payload: bencode_encode(&hello),
        })
        .await?;

        let mut size: Option<usize> = None;
        loop {
            let message = self.read_message().await?;
            if let Message::Extended { id: 0, payload } = message {
                let value = bencode_decode(&payload)?;
                let speaks = value
                    .get("m")
                    .and_then(|m| m.get("ut_metadata"))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                if speaks == 0 {
                    return Err(TorrentError::Unsupported(
                        "the peer does not have the metadata of the torrent".into(),
                    ));
                }
                size = value
                    .get("metadata_size")
                    .and_then(|v| v.as_int())
                    .map(|size| size as usize);
                break;
            }
        }
        let size = size.ok_or_else(|| {
            TorrentError::Unsupported("the peer does not have the metadata of the torrent".into())
        })?;

        let mut metadata = vec![0u8; size];
        let mut got = 0usize;
        while got < size {
            let request = BencodeValue::Dict(vec![
                ("msg_type".into(), BencodeValue::Integer(0)),
                ("piece".into(), BencodeValue::Integer((got / BLOCK_SIZE) as i64)),
            ]);
            self.send(&Message::Extended {
                id: 0,
                payload: bencode_encode(&request),
            })
            .await?;
            loop {
                let message = self.read_message().await?;
                if let Message::Extended { id: 1, payload } = message {
                    let value = bencode_decode(&payload)?;
                    let piece = value.get("piece").and_then(|v| v.as_int()).unwrap_or(0) as usize;
                    let data = value.get("data").and_then(|v| v.as_bytes()).unwrap_or(&[]);
                    let at = piece * BLOCK_SIZE;
                    if at + data.len() <= metadata.len() {
                        metadata[at..at + data.len()].copy_from_slice(data);
                        got += data.len();
                    }
                    break;
                }
            }
        }
        Ok(metadata)
    }

    /// Give the peer the block it asked for.
    pub async fn serve_request(
        &mut self,
        index: u32,
        begin: u32,
        length: u32,
    ) -> Result<(), TorrentError> {
        let Some(source) = self.source.clone() else {
            return Ok(());
        };
        let Some(bytes) = source.read_block(index, begin, length).await else {
            return Ok(());
        };
        let size = bytes.len() as u64;
        self.send(&Message::Piece {
            index,
            begin,
            data: bytes,
        })
        .await?;
        self.uploaded += size;
        Ok(())
    }

    fn set_have(&mut self, index: u32) {
        if let Some(slot) = self.have.get_mut(index as usize) {
            *slot = true;
        }
    }

    fn set_bitfield(&mut self, bytes: &[u8]) {
        for (index, slot) in self.have.iter_mut().enumerate() {
            let byte = bytes.get(index / 8).copied().unwrap_or(0);
            *slot = byte & (1 << (7 - index % 8)) != 0;
        }
    }
}
