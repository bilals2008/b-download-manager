//! KRPC: the messages of the distributed hash table, as they go over UDP.
//!
//! The format is the one the specification of the table describes: every
//! message is a bencoded dictionary with a two-byte transaction id, a type
//! and either the arguments of a question or the body of an answer.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::bencode::{bencode_decode, bencode_encode, BencodeValue};
use crate::error::TorrentError;

/// The id of a node: twenty bytes.
pub type NodeId = [u8; 20];

/// The four questions the table speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// Are you there?
    Ping,
    /// Who are the nodes nearest to this id?
    FindNode,
    /// Who has the peers of this info hash?
    GetPeers,
    /// I have this piece of this info hash.
    AnnouncePeer,
}

impl Query {
    /// The name the question goes by on the wire.
    pub fn name(&self) -> &'static str {
        match self {
            Query::Ping => "ping",
            Query::FindNode => "find_node",
            Query::GetPeers => "get_peers",
            Query::AnnouncePeer => "announce_peer",
        }
    }

    /// The question a name stands for, when it is one this table speaks.
    pub fn from_name(name: &str) -> Option<Query> {
        match name {
            "ping" => Some(Query::Ping),
            "find_node" => Some(Query::FindNode),
            "get_peers" => Some(Query::GetPeers),
            "announce_peer" => Some(Query::AnnouncePeer),
            _ => None,
        }
    }
}

/// A message that came in.
#[derive(Clone, Debug)]
pub enum Message {
    /// A question from another node.
    Query {
        transaction: Vec<u8>,
        query: Query,
        arguments: Vec<(String, BencodeValue)>,
    },
    /// The answer to a question.
    Response {
        transaction: Vec<u8>,
        body: Vec<(String, BencodeValue)>,
    },
    /// The refusal of a question.
    Error {
        transaction: Vec<u8>,
        #[cfg_attr(not(test), allow(dead_code))]
        code: i64,
        #[cfg_attr(not(test), allow(dead_code))]
        reason: String,
    },
}

/// A question, as it goes on the wire.
pub fn encode_query(
    transaction: &[u8],
    query: Query,
    arguments: Vec<(String, BencodeValue)>,
) -> Vec<u8> {
    bencode_encode(&BencodeValue::Dict(vec![
        ("t".into(), BencodeValue::Bytes(transaction.to_vec())),
        ("y".into(), BencodeValue::Bytes(b"q".to_vec())),
        (
            "q".into(),
            BencodeValue::Bytes(query.name().as_bytes().to_vec()),
        ),
        ("a".into(), BencodeValue::Dict(arguments)),
    ]))
}

/// The answer to a question.
pub fn encode_response(transaction: &[u8], body: Vec<(String, BencodeValue)>) -> Vec<u8> {
    bencode_encode(&BencodeValue::Dict(vec![
        ("t".into(), BencodeValue::Bytes(transaction.to_vec())),
        ("y".into(), BencodeValue::Bytes(b"r".to_vec())),
        ("r".into(), BencodeValue::Dict(body)),
    ]))
}

/// The refusal of a question. The code is the one the specification names.
pub fn encode_error(transaction: &[u8], code: i64, reason: &str) -> Vec<u8> {
    bencode_encode(&BencodeValue::Dict(vec![
        ("t".into(), BencodeValue::Bytes(transaction.to_vec())),
        ("y".into(), BencodeValue::Bytes(b"e".to_vec())),
        (
            "e".into(),
            BencodeValue::List(vec![
                BencodeValue::Integer(code),
                BencodeValue::Bytes(reason.as_bytes().to_vec()),
            ]),
        ),
    ]))
}

/// Read a message that came in.
pub fn decode(bytes: &[u8]) -> Result<Message, TorrentError> {
    let value = bencode_decode(bytes)?;
    let BencodeValue::Dict(entries) = value else {
        return Err(TorrentError::Malformed(
            "the message of the table is not a dictionary".into(),
        ));
    };
    let find = |key: &str| {
        entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    };
    let transaction = match find("t") {
        Some(BencodeValue::Bytes(bytes)) => bytes.clone(),
        _ => {
            return Err(TorrentError::Malformed(
                "the message of the table has no transaction id".into(),
            ))
        }
    };
    let kind = match find("y") {
        Some(BencodeValue::Bytes(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        _ => {
            return Err(TorrentError::Malformed(
                "the message of the table has no type".into(),
            ))
        }
    };
    match kind.as_str() {
        "q" => {
            let name = match find("q") {
                Some(BencodeValue::Bytes(bytes)) => String::from_utf8_lossy(bytes).to_string(),
                _ => {
                    return Err(TorrentError::Malformed(
                        "the question of the table has no name".into(),
                    ))
                }
            };
            let query = Query::from_name(&name).ok_or_else(|| {
                TorrentError::Unsupported(format!("the table does not speak {name}"))
            })?;
            let arguments = match find("a") {
                Some(BencodeValue::Dict(entries)) => entries.clone(),
                _ => Vec::new(),
            };
            Ok(Message::Query {
                transaction,
                query,
                arguments,
            })
        }
        "r" => {
            let body = match find("r") {
                Some(BencodeValue::Dict(entries)) => entries.clone(),
                _ => {
                    return Err(TorrentError::Malformed(
                        "the answer of the table has no body".into(),
                    ))
                }
            };
            Ok(Message::Response { transaction, body })
        }
        "e" => {
            let (code, reason) = match find("e") {
                Some(BencodeValue::List(values)) if values.len() == 2 => {
                    let code = match &values[0] {
                        BencodeValue::Integer(code) => *code,
                        _ => 201,
                    };
                    let reason = match &values[1] {
                        BencodeValue::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
                        _ => String::new(),
                    };
                    (code, reason)
                }
                _ => (201, String::new()),
            };
            Ok(Message::Error {
                transaction,
                code,
                reason,
            })
        }
        _ => Err(TorrentError::Malformed(
            "the type of the message is not one this table speaks".into(),
        )),
    }
}

/// The nodes of an answer, as they go on the wire: twenty bytes of id, four
/// of address and two of port, for each of them.
pub fn encode_nodes(nodes: &[(NodeId, SocketAddr)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nodes.len() * 26);
    for (id, address) in nodes {
        out.extend_from_slice(id);
        out.extend_from_slice(&match address.ip() {
            IpAddr::V4(v4) => v4.octets(),
            IpAddr::V6(_) => [0; 4],
        });
        out.extend_from_slice(&address.port().to_be_bytes());
    }
    out
}

/// The nodes of an answer, read back.
pub fn decode_nodes(bytes: &[u8]) -> Vec<(NodeId, SocketAddr)> {
    bytes
        .as_chunks::<26>()
        .0
        .iter()
        .map(|chunk| {
            let mut id = [0u8; 20];
            id.copy_from_slice(&chunk[0..20]);
            let address = Ipv4Addr::new(chunk[20], chunk[21], chunk[22], chunk[23]);
            let port = u16::from_be_bytes([chunk[24], chunk[25]]);
            (id, SocketAddr::new(IpAddr::V4(address), port))
        })
        .collect()
}

/// The peers of an answer, as they go on the wire: four bytes of address and
/// two of port, for each of them.
pub fn encode_peers(peers: &[SocketAddr]) -> Vec<u8> {
    let mut out = Vec::with_capacity(peers.len() * 6);
    for address in peers {
        let octets = match address.ip() {
            IpAddr::V4(v4) => v4.octets(),
            IpAddr::V6(_) => [0; 4],
        };
        out.extend_from_slice(&octets);
        out.extend_from_slice(&address.port().to_be_bytes());
    }
    out
}

/// The peers of an answer, as the specification sends them: a list of
/// six-byte strings, one peer in each.
pub fn encode_peer_values(peers: &[SocketAddr]) -> BencodeValue {
    BencodeValue::List(
        peers
            .iter()
            .map(|peer| BencodeValue::Bytes(encode_peers(std::slice::from_ref(peer))))
            .collect(),
    )
}

/// The peers of an answer, read back.
pub fn decode_peers(bytes: &[u8]) -> Vec<SocketAddr> {
    bytes
        .as_chunks::<6>()
        .0
        .iter()
        .map(|chunk| {
            let address = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            SocketAddr::new(IpAddr::V4(address), port)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_and_its_answer_are_read() {
        let bytes = encode_query(
            b"aa",
            Query::GetPeers,
            vec![
                ("id".into(), BencodeValue::Bytes(vec![1; 20])),
                ("info_hash".into(), BencodeValue::Bytes(vec![2; 20])),
            ],
        );
        let Message::Query {
            transaction,
            query,
            arguments,
        } = decode(&bytes).expect("the question")
        else {
            panic!("a question was expected");
        };
        assert_eq!(transaction, b"aa");
        assert_eq!(query, Query::GetPeers);
        assert_eq!(arguments.len(), 2);

        let bytes = encode_response(b"bb", vec![("id".into(), BencodeValue::Bytes(vec![3; 20]))]);
        let Message::Response { transaction, body } = decode(&bytes).expect("the answer") else {
            panic!("an answer was expected");
        };
        assert_eq!(transaction, b"bb");
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn a_refusal_is_read() {
        let bytes = encode_error(b"cc", 203, "the token is not one of ours");
        let Message::Error {
            transaction,
            code,
            reason,
        } = decode(&bytes).expect("the refusal")
        else {
            panic!("a refusal was expected");
        };
        assert_eq!(transaction, b"cc");
        assert_eq!(code, 203);
        assert!(reason.contains("token"));
    }

    #[test]
    fn a_question_this_table_does_not_speak_is_refused() {
        let bytes = bencode_encode(&BencodeValue::Dict(vec![
            ("t".into(), BencodeValue::Bytes(b"dd".to_vec())),
            ("y".into(), BencodeValue::Bytes(b"q".to_vec())),
            ("q".into(), BencodeValue::Bytes(b"vote".to_vec())),
            ("a".into(), BencodeValue::Dict(vec![])),
        ]));
        assert!(decode(&bytes).is_err());
    }

    #[test]
    fn the_nodes_and_the_peers_go_and_come_back() {
        let nodes = vec![
            ([1u8; 20], "10.0.0.1:6881".parse().expect("the address")),
            ([2u8; 20], "192.168.0.5:51413".parse().expect("the address")),
        ];
        let bytes = encode_nodes(&nodes);
        assert_eq!(bytes.len(), 52);
        assert_eq!(decode_nodes(&bytes), nodes);

        let peers = vec!["8.8.8.8:1234".parse().expect("the address")];
        let bytes = encode_peers(&peers);
        assert_eq!(bytes.len(), 6);
        assert_eq!(decode_peers(&bytes), peers);
    }
}
