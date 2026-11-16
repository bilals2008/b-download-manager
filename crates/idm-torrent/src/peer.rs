//! The wire between two peers: the handshake and the messages that follow
//! it.

use serde::Serialize;

use crate::error::TorrentError;

/// The name every peer says at the start of a connection.
pub const PROTOCOL: &[u8] = b"BitTorrent protocol";

/// How many bytes a block holds.
pub const BLOCK_SIZE: usize = 16 * 1024;

/// The first thing two peers say to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handshake {
    /// The eight reserved bytes; the fifth one says the peer speaks the
    /// extension protocol.
    pub reserved: [u8; 8],
    /// The info hash of the torrent.
    pub info_hash: [u8; 20],
    /// The peer id of the other side.
    pub peer_id: [u8; 20],
}

impl Handshake {
    /// A handshake for a torrent, from a peer.
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Handshake {
        let mut reserved = [0u8; 8];
        // The extension protocol, which is how the metadata of a magnet is
        // asked for.
        reserved[5] = 0x10;
        Handshake {
            reserved,
            info_hash,
            peer_id,
        }
    }

    /// The 68 bytes of the handshake.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(68);
        out.push(PROTOCOL.len() as u8);
        out.extend_from_slice(PROTOCOL);
        out.extend_from_slice(&self.reserved);
        out.extend_from_slice(&self.info_hash);
        out.extend_from_slice(&self.peer_id);
        out
    }

    /// Read a handshake.
    pub fn decode(bytes: &[u8]) -> Result<Handshake, TorrentError> {
        if bytes.len() < 68 {
            return Err(TorrentError::Malformed(
                "the handshake is too short".into(),
            ));
        }
        if &bytes[1..20] != PROTOCOL {
            return Err(TorrentError::Malformed(
                "the peer does not speak the BitTorrent protocol".into(),
            ));
        }
        let mut reserved = [0u8; 8];
        reserved.copy_from_slice(&bytes[20..28]);
        let mut info_hash = [0u8; 20];
        info_hash.copy_from_slice(&bytes[28..48]);
        let mut peer_id = [0u8; 20];
        peer_id.copy_from_slice(&bytes[48..68]);
        Ok(Handshake {
            reserved,
            info_hash,
            peer_id,
        })
    }

    /// Whether the peer speaks the extension protocol.
    pub fn speaks_extensions(&self) -> bool {
        self.reserved[5] & 0x10 != 0
    }
}

/// A message a peer sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// The peer will not send anything.
    Choke,
    /// The peer will send what is asked for.
    Unchoke,
    /// We want what the peer has.
    Interested,
    /// We do not want what the peer has.
    NotInterested,
    /// The peer has a piece.
    Have(u32),
    /// The peer has these pieces.
    Bitfield(Vec<u8>),
    /// A block of a piece is asked for.
    Request {
        /// The piece.
        index: u32,
        /// Where the block starts inside the piece.
        begin: u32,
        /// How many bytes the block holds.
        length: u32,
    },
    /// A block of a piece.
    Piece {
        /// The piece.
        index: u32,
        /// Where the block starts inside the piece.
        begin: u32,
        /// The bytes of the block.
        data: Vec<u8>,
    },
    /// A block that is not wanted after all.
    Cancel {
        /// The piece.
        index: u32,
        /// Where the block starts inside the piece.
        begin: u32,
        /// How many bytes the block holds.
        length: u32,
    },
    /// The port the peer listens on.
    Port(u16),
    /// A message of the extension protocol.
    Extended {
        /// Which extension it is.
        id: u8,
        /// The bytes of the message.
        payload: Vec<u8>,
    },
    /// Nothing, sent to say that the connection is still there.
    KeepAlive,
    /// A message this crate does not act on; kept, not mistaken for a failure.
    Unknown {
        /// The id the peer used.
        id: u8,
        /// The bytes behind it.
        payload: Vec<u8>,
    },
}

impl Message {
    /// The bytes of the message, without the length in front.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Message::Choke => vec![0],
            Message::Unchoke => vec![1],
            Message::Interested => vec![2],
            Message::NotInterested => vec![3],
            Message::Have(index) => {
                let mut out = vec![4];
                out.extend_from_slice(&index.to_be_bytes());
                out
            }
            Message::Bitfield(bytes) => {
                let mut out = vec![5];
                out.extend_from_slice(bytes);
                out
            }
            Message::Request {
                index,
                begin,
                length,
            }
            => {
                let mut out = vec![6];
                out.extend_from_slice(&index.to_be_bytes());
                out.extend_from_slice(&begin.to_be_bytes());
                out.extend_from_slice(&length.to_be_bytes());
                out
            }
            Message::Piece {
                index,
                begin,
                data,
            } => {
                let mut out = vec![7];
                out.extend_from_slice(&index.to_be_bytes());
                out.extend_from_slice(&begin.to_be_bytes());
                out.extend_from_slice(data);
                out
            }
            Message::Cancel {
                index,
                begin,
                length,
            } => {
                let mut out = vec![8];
                out.extend_from_slice(&index.to_be_bytes());
                out.extend_from_slice(&begin.to_be_bytes());
                out.extend_from_slice(&length.to_be_bytes());
                out
            }
            Message::Port(port) => {
                let mut out = vec![9];
                out.extend_from_slice(&port.to_be_bytes());
                out
            }
            Message::Extended { id, payload } => {
                let mut out = vec![20, *id];
                out.extend_from_slice(payload);
                out
            }
            Message::KeepAlive => Vec::new(),
            Message::Unknown { id, payload } => {
                let mut out = vec![*id];
                out.extend_from_slice(payload);
                out
            }
        }
    }

    /// The bytes of the message with the length in front, the way it goes
    /// over the wire.
    pub fn to_wire(&self) -> Vec<u8> {
        let body = self.encode();
        let mut out = Vec::with_capacity(body.len() + 4);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// Read a message from its id and its bytes.
    pub fn decode(id: u8, payload: &[u8]) -> Result<Message, TorrentError> {
        let need = |want: usize| -> Result<(), TorrentError> {
            if payload.len() < want {
                return Err(TorrentError::Malformed(
                    "a message is shorter than it says".into(),
                ));
            }
            Ok(())
        };
        let number = |at: usize| -> u32 {
            u32::from_be_bytes([
                payload[at],
                payload[at + 1],
                payload[at + 2],
                payload[at + 3],
            ])
        };
        Ok(match id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => {
                need(4)?;
                Message::Have(number(0))
            }
            5 => Message::Bitfield(payload.to_vec()),
            6 => {
                need(12)?;
                Message::Request {
                    index: number(0),
                    begin: number(4),
                    length: number(8),
                }
            }
            7 => {
                need(8)?;
                Message::Piece {
                    index: number(0),
                    begin: number(4),
                    data: payload[8..].to_vec(),
                }
            }
            8 => {
                need(12)?;
                Message::Cancel {
                    index: number(0),
                    begin: number(4),
                    length: number(8),
                }
            }
            9 => {
                need(2)?;
                Message::Port(u16::from_be_bytes([payload[0], payload[1]]))
            }
            20 => {
                need(1)?;
                Message::Extended {
                    id: payload[0],
                    payload: payload[1..].to_vec(),
                }
            }
            other => {
                Message::Unknown {
                    id: other,
                    payload: payload.to_vec(),
                }
            }
        })
    }
}

/// A peer of a job, as it is shown.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    /// The address of the peer.
    pub address: String,
    /// The id the peer says.
    pub peer_id: String,
    /// The program the peer runs, when its id says so.
    pub client: String,
    /// Whether the connection is there now.
    pub connected: bool,
    /// How many bytes came from this peer.
    pub downloaded: u64,
    /// How many bytes went to this peer.
    pub uploaded: u64,
    /// How much of the torrent this peer has, from 0 to 1.
    pub progress: f32,
    /// How many bytes a second come from this peer.
    pub down_speed: u64,
    /// How many bytes a second go to this peer.
    pub up_speed: u64,
}

/// The program a peer runs, read from its peer id the Azureus way: the first
/// eight bytes of the id carry the name of the client, when the client puts
/// its name there. An id that says nothing gives nothing.
pub fn client_name(peer_id: &[u8; 20]) -> String {
    let head = &peer_id[..8];
    if !head.iter().all(|byte| byte.is_ascii_graphic()) {
        return String::new();
    }
    let text = String::from_utf8_lossy(head);
    text.trim_matches(|character: char| !character.is_ascii_alphanumeric())
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handshake_goes_out_and_comes_back() {
        let handshake = Handshake::new([7; 20], [9; 20]);
        let bytes = handshake.encode();
        assert_eq!(bytes.len(), 68);
        assert_eq!(Handshake::decode(&bytes).unwrap(), handshake);
        assert!(handshake.speaks_extensions());
    }

    #[test]
    fn a_handshake_of_another_protocol_is_refused() {
        let mut bytes = Handshake::new([7; 20], [9; 20]).encode();
        bytes[1] = b'X';
        assert!(Handshake::decode(&bytes).is_err());
        assert!(Handshake::decode(&[0u8; 10]).is_err());
    }

    #[test]
    fn a_message_goes_out_and_comes_back() {
        let messages = vec![
            Message::Choke,
            Message::Unchoke,
            Message::Interested,
            Message::Have(7),
            Message::Bitfield(vec![0b1010_1010, 0b0101_0101]),
            Message::Request {
                index: 3,
                begin: 16384,
                length: 16384,
            },
            Message::Piece {
                index: 3,
                begin: 0,
                data: vec![1, 2, 3],
            },
            Message::Port(6881),
            Message::Extended {
                id: 0,
                payload: vec![9],
            },
        ];
        for message in messages {
            let body = message.encode();
            assert_eq!(Message::decode(body[0], &body[1..]).unwrap(), message);
        }
    }

    #[test]
    fn a_keep_alive_is_nothing() {
        // A keep-alive on the wire is a four-byte length of zero, nothing else.
        assert_eq!(Message::KeepAlive.to_wire(), vec![0, 0, 0, 0]);
        assert_eq!(Message::KeepAlive.encode(), Vec::<u8>::new());
    }

    #[test]
    fn the_name_of_a_client_is_read_from_its_peer_id() {
        let mut id = [0u8; 20];
        id[..8].copy_from_slice(b"-qB5050-");
        assert_eq!(client_name(&id), "qB5050");

        let mut id = [0u8; 20];
        id[..8].copy_from_slice(b"qBittorr");
        assert_eq!(client_name(&id), "qBittorr");
    }

    #[test]
    fn a_peer_id_that_says_nothing_gives_no_client() {
        // A hash of bytes, like the one this crate makes for itself.
        let id = [0x9fu8; 20];
        assert_eq!(client_name(&id), "");
    }
}
