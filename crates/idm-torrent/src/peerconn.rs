//! A connection to one peer: the handshake, the messages that go over it and
//! the pieces that come back.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// Whether the peer speaks the extension protocol (BEP 10).
    extension: bool,
    piece_count: u64,
    have: Vec<bool>,
    choked: bool,
    interested: bool,
    /// How many bytes came from this peer.
    pub downloaded: u64,
    /// How many bytes went to this peer.
    pub uploaded: u64,
    source: Option<Arc<dyn BlockSource>>,
    /// How many bytes a second may go out; zero is no limit.
    upload_limit: u64,
    /// When the window of the upload started.
    window_start: Instant,
    /// How many bytes went out in this window.
    window_bytes: u64,
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
            extension: false,
            piece_count,
            have: vec![false; piece_count as usize],
            choked: false,
            interested: false,
            downloaded: 0,
            uploaded: 0,
            source: None,
            upload_limit: 0,
            window_start: Instant::now(),
            window_bytes: 0,
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
        session.extension = handshake.speaks_extensions();
        session.read_initial().await?;
        Ok(session)
    }

    /// Read what the peer says before anything else, so its bitfield is
    /// known before a piece is asked for. Peers answer hello with the
    /// bitfield; a peer that sends nothing is kept, it just has nothing
    /// this job can use yet.
    async fn read_initial(&mut self) -> Result<(), TorrentError> {
        let window = Duration::from_secs(5);
        let deadline = std::time::Instant::now() + window;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            match tokio::time::timeout(left, self.read_message()).await {
                // The bitfield is the answer; the rest can wait for the
                // exchange that actually needs it.
                Ok(Ok(Message::Bitfield(bytes))) => {
                    self.set_bitfield(&bytes);
                    return Ok(());
                }
                Ok(Ok(Message::Have(piece))) => self.set_have(piece),
                Ok(Ok(Message::Unchoke)) => self.choked = false,
                Ok(Ok(Message::Choke)) => self.choked = true,
                Ok(Ok(Message::Request {
                    index,
                    begin,
                    length,
                })) => {
                    self.serve_request(index, begin, length).await?;
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => return Err(error),
                Err(_) => return Ok(()),
            }
        }
    }

    /// Where the blocks of a piece come from once the job has them.
    pub fn set_source(&mut self, source: Arc<dyn BlockSource>) {
        self.source = Some(source);
    }

    /// Keep the upload under the limit the reader set; zero is no limit.
    pub fn set_upload_limit(&mut self, limit: u64) {
        if limit != self.upload_limit {
            self.upload_limit = limit;
            self.window_start = Instant::now();
            self.window_bytes = 0;
        }
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
                Message::Unknown { .. } => {}
            }
        }
        Ok(data)
    }

    /// Ask the peer for the metadata of the torrent (BEP 9).
    pub async fn fetch_metadata(&mut self) -> Result<Vec<u8>, TorrentError> {
        /// The largest metadata this crate will hold in memory. A torrent of a
        /// hundred thousand files is a couple of megabytes; past this the
        /// peer is asking for a memory allocation this machine cannot honour.
        const MAX_METADATA: usize = 16 * 1024 * 1024;
        if !self.extension {
            return Err(TorrentError::Unsupported(
                "the peer does not speak the extension protocol".into(),
            ));
        }
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

        // The id the peer wants metadata messages sent to it under. It is in
        // the peer's own handshake and is not always 1, so a request sent to
        // the handshake id — or to this crate's own id — is one the peer does
        // not read, and the metadata never comes.
        let to_the_peer;
        let size = loop {
            match self.read_message().await? {
                Message::Extended { id: 0, payload } => {
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
                    to_the_peer = speaks as u8;
                    break value
                        .get("metadata_size")
                        .and_then(|v| v.as_int())
                        .map(|size| size as usize);
                }
                other => self.handle_control(other).await?,
            }
        }
        .ok_or_else(|| {
            TorrentError::Unsupported("the peer does not have the metadata of the torrent".into())
        })?;
        if size > MAX_METADATA {
            return Err(TorrentError::Unsupported(
                "the metadata of the torrent is larger than this machine will hold".into(),
            ));
        }

        let mut metadata = vec![0u8; size];
        let mut got = 0usize;
        while got < size {
            let request = BencodeValue::Dict(vec![
                ("msg_type".into(), BencodeValue::Integer(0)),
                ("piece".into(), BencodeValue::Integer((got / BLOCK_SIZE) as i64)),
            ]);
            self.send(&Message::Extended {
                id: to_the_peer,
                payload: bencode_encode(&request),
            })
            .await?;
            loop {
                match self.read_message().await? {
                    Message::Extended { id: 1, payload } => {
                        let (value, consumed) =
                            crate::bencode::decode_value_with_end(&payload)?;
                        if value.get("msg_type").and_then(|v| v.as_int()) == Some(2) {
                            return Err(TorrentError::Network(
                                "the peer rejected the metadata request".into(),
                            ));
                        }
                        let piece = value.get("piece").and_then(|v| v.as_int()).unwrap_or(0) as usize;
                        // Some clients put the raw piece bytes straight after
                        // the dictionary instead of in a `data` key.
                        let data: Vec<u8> = match value
                            .get("data")
                            .and_then(|v| v.as_bytes())
                            .map(<[u8]>::to_vec)
                        {
                            Some(data) => data,
                            None => payload[consumed..].to_vec(),
                        };
                        if data.is_empty() {
                            return Err(TorrentError::Network(
                                "the peer sent no bytes for the piece".into(),
                            ));
                        }
                        let at = piece * BLOCK_SIZE;
                        if at + data.len() <= metadata.len() {
                            metadata[at..at + data.len()].copy_from_slice(&data);
                            got += data.len();
                        }
                        break;
                    }
                    other => self.handle_control(other).await?,
                }
            }
        }
        Ok(metadata)
    }

    /// The messages that are not the exchange at hand: what the peer has,
    /// what it wants, and a block it is owed.
    async fn handle_control(&mut self, message: Message) -> Result<(), TorrentError> {
        match message {
            Message::Have(piece) => {
                self.set_have(piece);
                Ok(())
            }
            Message::Bitfield(bytes) => {
                self.set_bitfield(&bytes);
                Ok(())
            }
            Message::Unchoke => {
                self.choked = false;
                Ok(())
            }
            Message::Choke => {
                self.choked = true;
                Ok(())
            }
            Message::Request {
                index,
                begin,
                length,
            } => self.serve_request(index, begin, length).await,
            _ => Ok(()),
        }
    }

    /// Give the peer the block it asked for.
    pub async fn serve_request(
        &mut self,
        index: u32,
        begin: u32,
        length: u32,
    ) -> Result<(), TorrentError> {
        /// The largest block this crate will send to a peer; a block larger
        /// than this is never a legitimate request.
        const MAX_BLOCK: u32 = 1 << 20;
        if length == 0 || length > MAX_BLOCK {
            return Ok(());
        }
        let Some(source) = self.source.clone() else {
            return Ok(());
        };
        let Some(bytes) = source.read_block(index, begin, length).await else {
            return Ok(());
        };
        let size = bytes.len() as u64;
        self.pace_upload(size).await;
        self.send(&Message::Piece {
            index,
            begin,
            data: bytes,
        })
        .await?;
        self.uploaded += size;
        Ok(())
    }

    /// Wait until a block may go out. The limit is a number of bytes a
    /// second, so a block waits for the window to turn over when the window
    /// is full. The limit is not exact; it keeps the upload near it.
    async fn pace_upload(&mut self, amount: u64) {
        if self.upload_limit == 0 {
            self.window_bytes += amount;
            return;
        }
        loop {
            let elapsed = self.window_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                self.window_start = Instant::now();
                self.window_bytes = 0;
                break;
            }
            if self.window_bytes + amount <= self.upload_limit {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1) - elapsed).await;
        }
        self.window_bytes += amount;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The peer of a torrent: it answers hello, then says what it has, then
    /// waits. The test drives the client against it over a loopback socket.
    #[tokio::test]
    async fn a_peer_names_the_pieces_it_has_before_any_piece_is_asked() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let info_hash = [7u8; 20];
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut hello = vec![19u8];
            hello.extend_from_slice(b"BitTorrent protocol");
            let mut reserved = [0u8; 8];
            reserved[5] = 0x10;
            hello.extend_from_slice(&reserved);
            hello.extend_from_slice(&info_hash);
            hello.extend_from_slice(&[9u8; 20]);
            socket.write_all(&hello).await.unwrap();
            let bitfield = Message::to_wire(&Message::Bitfield(vec![0b1010_0000]));
            socket.write_all(&bitfield).await.unwrap();
            // Hold the connection open so the read succeeds.
            std::future::pending::<()>().await;
        });

        let session = PeerSession::connect(address, info_hash, [8u8; 20], 3)
            .await
            .expect("the peer said hello");
        assert!(session.has_piece(0));
        assert!(!session.has_piece(1));
        assert!(session.has_piece(2));
        assert!(session.extension);
    }

    /// The metadata message of a common client carries its bytes straight
    /// after the dictionary instead of in a `data` key. The client must not
    /// wait forever for a key that is never coming.
    #[tokio::test]
    async fn metadata_whose_bytes_follow_the_dictionary_is_read() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let metadata = b"d1:a4:wiki4:minee".to_vec();
        let size = metadata.len();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let info_hash = [7u8; 20];
        let script = metadata.clone();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            async fn message(socket: &mut tokio::net::TcpStream, body: Vec<u8>) {
                let length = (body.len() as u32).to_be_bytes();
                socket.write_all(&length).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }
            async fn read(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
                let mut length_prefix = [0u8; 4];
                socket.read_exact(&mut length_prefix).await.unwrap();
                let length = u32::from_be_bytes(length_prefix) as usize;
                let mut body = vec![0u8; length];
                socket.read_exact(&mut body).await.unwrap();
                body
            }
            // The handshake: their bytes in, our bytes out.
            let mut hello = vec![19u8];
            hello.extend_from_slice(b"BitTorrent protocol");
            let mut reserved = [0u8; 8];
            reserved[5] = 0x10;
            hello.extend_from_slice(&reserved);
            hello.extend_from_slice(&info_hash);
            hello.extend_from_slice(&[9u8; 20]);
            let mut received = vec![0u8; 68];
            socket.read_exact(&mut received).await.unwrap();
            socket.write_all(&hello).await.unwrap();
            let bitfield = Message::to_wire(&Message::Bitfield(vec![0b1000_0000]));
            socket.write_all(&bitfield).await.unwrap();
            // The extension handshake.
            let theirs = read(&mut socket).await;
            assert_eq!(theirs[0], 20);
            let mut reply = vec![20, 0];
            reply.extend_from_slice(&crate::bencode::bencode_encode(&crate::bencode::BencodeValue::Dict(vec![
                (
                    "m".into(),
                    crate::bencode::BencodeValue::Dict(vec![(
                        "ut_metadata".into(),
                        crate::bencode::BencodeValue::Integer(1),
                    )]),
                ),
                ("metadata_size".into(), crate::bencode::BencodeValue::Integer(size as i64)),
            ])));
            message(&mut socket, reply).await;
            // The piece request, then the piece: a dictionary in front of the
            // bytes, and the bytes behind it.
            let request = read(&mut socket).await;
            assert_eq!(request[0], 20);
            let mut piece = vec![20, 1];
            piece.extend_from_slice(format!("d8:msg_typei1e5:piecei0e10:total_sizei{size}ee").as_bytes());
            piece.extend_from_slice(&script);
            message(&mut socket, piece).await;
            std::future::pending::<()>().await;
        });

        let mut session = PeerSession::connect(address, info_hash, [8u8; 20], 1)
            .await
            .expect("the peer said hello");
        let found = session
            .fetch_metadata()
            .await
            .expect("the metadata came back");
        assert_eq!(found, metadata);
    }
}
