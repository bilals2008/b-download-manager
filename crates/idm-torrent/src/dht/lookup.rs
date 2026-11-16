//! The walk of the table: the questions asked to find the peers of a hash.
//!
//! The walk is iterative and bounded: it starts from the nodes it knows,
//! asks the nearest ones first, and takes the nodes they name in their place
//! for the next round. It stops when there is nothing left to ask, when the
//! time is up, or when it has asked as many nodes as it is allowed to.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use crate::bencode::BencodeValue;
use crate::dht::krpc::{decode_nodes, decode_peers, NodeId};

/// How many nodes a walk asks.
pub const MAX_QUERIES: usize = 24;

/// How long a walk may take.
pub const DEADLINE: Duration = Duration::from_secs(20);

/// How long a walk that has no node to ask yet waits for the table to learn
/// one. A walk started right after the table itself started would otherwise
/// end before the first bootstrap answer came in.
const GRACE: Duration = Duration::from_secs(5);

/// The walk after the peers of an info hash.
pub struct Lookup {
    /// The id of this node, which every question carries.
    local: NodeId,
    /// The info hash that is looked for.
    target: [u8; 20],
    /// The port to announce, when this walk also announces this client.
    announce: Option<u16>,
    /// The nodes that may be asked, the nearest first.
    candidates: Vec<(NodeId, SocketAddr)>,
    /// The nodes that were asked already.
    queried: HashSet<SocketAddr>,
    /// The peers that answered.
    peers: Vec<SocketAddr>,
    /// The tokens the nodes gave, with the node that gave them.
    tokens: Vec<(SocketAddr, Vec<u8>)>,
    /// How many nodes were asked.
    asked: usize,
    /// When the walk gives up.
    deadline: Instant,
    /// Until when a walk without a node to ask waits for one.
    grace: Instant,
    /// Where the peers are sent when the walk is over.
    reply: Option<oneshot::Sender<Vec<SocketAddr>>>,
}

impl Lookup {
    /// A walk for an info hash, starting from the nodes that are known.
    pub fn new(
        local: NodeId,
        target: [u8; 20],
        announce: Option<u16>,
        seeds: Vec<(NodeId, SocketAddr)>,
        reply: oneshot::Sender<Vec<SocketAddr>>,
    ) -> Lookup {
        let mut lookup = Lookup {
            local,
            target,
            announce,
            candidates: Vec::new(),
            queried: HashSet::new(),
            peers: Vec::new(),
            tokens: Vec::new(),
            asked: 0,
            deadline: Instant::now() + DEADLINE,
            grace: Instant::now() + GRACE,
            reply: Some(reply),
        };
        for (id, address) in seeds {
            lookup.consider(id, address);
        }
        lookup
    }

    /// The next question to ask, when there is one to ask.
    pub fn next(&mut self) -> Option<(SocketAddr, Vec<(String, BencodeValue)>)> {
        if self.finished() {
            return None;
        }
        let index = self
            .candidates
            .iter()
            .position(|(_, address)| !self.queried.contains(address))?;
        let (_, address) = self.candidates.remove(index);
        self.queried.insert(address);
        self.asked += 1;
        let arguments = vec![
            ("id".into(), BencodeValue::Bytes(self.local.to_vec())),
            ("info_hash".into(), BencodeValue::Bytes(self.target.to_vec())),
        ];
        Some((address, arguments))
    }

    /// The info hash the walk is after.
    pub fn hash(&self) -> [u8; 20] {
        self.target
    }

    /// The answer of a node, with the peers and the nodes it names.
    pub fn answer(&mut self, node: SocketAddr, body: &[(String, BencodeValue)]) {
        let find = |key: &str| {
            body.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        };
        if let Some(BencodeValue::Bytes(bytes)) = find("token") {
            self.tokens.push((node, bytes.clone()));
        }
        // The specification sends the peers as a list of six-byte strings;
        // some clients send one long string of them instead. Both are read.
        match find("values") {
            Some(BencodeValue::Bytes(bytes)) => {
                for peer in decode_peers(bytes) {
                    if !self.peers.contains(&peer) {
                        self.peers.push(peer);
                    }
                }
            }
            Some(BencodeValue::List(values)) => {
                for value in values {
                    if let BencodeValue::Bytes(bytes) = value {
                        for peer in decode_peers(bytes) {
                            if !self.peers.contains(&peer) {
                                self.peers.push(peer);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(BencodeValue::Bytes(bytes)) = find("nodes") {
            for (id, address) in decode_nodes(bytes) {
                self.consider(id, address);
            }
        }
    }

    /// Whether the walk is over. A walk that has not asked anyone yet is
    /// given the grace to wait for a node to turn up; after that, an empty
    /// list of candidates is the end of the walk. The walk goes on while
    /// there are nearer nodes to ask, so it gathers the peers of the whole
    /// neighbourhood, not just the first answer.
    pub fn finished(&self) -> bool {
        self.asked >= MAX_QUERIES
            || Instant::now() >= self.deadline
            || (self.candidates.is_empty()
                && (self.asked > 0 || Instant::now() >= self.grace))
    }

    /// Whether the walk still waits for its first node, so the table can
    /// hand it the ones it has learned since the walk started.
    pub fn needs_seeds(&self) -> bool {
        self.asked == 0
    }

    /// Take the nodes the table knows now, nearest first.
    pub fn seed(&mut self, seeds: Vec<(NodeId, SocketAddr)>) {
        for (id, address) in seeds {
            self.consider(id, address);
        }
    }

    /// The peers the walk found.
    pub fn peers(&self) -> Vec<SocketAddr> {
        self.peers.clone()
    }

    /// The nodes that gave a token, with the token, for the announcement.
    pub fn announcements(&self) -> Vec<(SocketAddr, Vec<u8>)> {
        self.tokens.clone()
    }

    /// The port this walk announces, when it announces one.
    pub fn announce_port(&self) -> Option<u16> {
        self.announce
    }

    /// The answer the walk was asked for.
    pub fn take_reply(&mut self) -> Option<oneshot::Sender<Vec<SocketAddr>>> {
        self.reply.take()
    }

    /// Put a node into the list of nodes that may be asked, when it is
    /// nearer to the target than the ones already there. This node itself
    /// is never a candidate: it answers its own questions from its store.
    fn consider(&mut self, id: NodeId, address: SocketAddr) {
        if id == self.local || self.queried.contains(&address) {
            return;
        }
        if self
            .candidates
            .iter()
            .any(|(_, known)| *known == address)
        {
            return;
        }
        let position = self
            .candidates
            .iter()
            .position(|(known, _)| crate::dht::routing::is_closer(&id, known, &self.target));
        match position {
            Some(index) => self.candidates.insert(index, (id, address)),
            None => self.candidates.push((id, address)),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::BencodeValue;
    use crate::dht::krpc::{encode_nodes, encode_peers};

    fn an_id(seed: u8) -> NodeId {
        [seed; 20]
    }

    #[tokio::test]
    async fn a_walk_asks_the_nearest_node_first() {
        let (reply, answer) = oneshot::channel();
        let mut walk = Lookup::new(
            an_id(0),
            [1u8; 20],
            None,
            vec![
                (an_id(9), "10.0.0.9:6881".parse().expect("the address")),
                (an_id(1), "10.0.0.1:6881".parse().expect("the address")),
            ],
            reply,
        );
        let (address, _) = walk.next().expect("a question");
        assert_eq!(address.to_string(), "10.0.0.1:6881");
        walk.answer(
            address,
            &[("values".into(), BencodeValue::Bytes(encode_peers(&[
                "1.2.3.4:6881".parse().expect("the address"),
            ])))],
        );
        assert!(!walk.peers().is_empty());
        // The walk asks the rest of its candidates before it is over.
        let (next, _) = walk.next().expect("the second question");
        assert_eq!(next.to_string(), "10.0.0.9:6881");
        assert!(walk.next().is_none());
        assert!(walk.finished());
        let peers = walk.take_reply().expect("the answer");
        peers.send(walk.peers()).expect("the peers");
        assert_eq!(answer.await.expect("the answer").len(), 1);
    }

    #[tokio::test]
    async fn a_walk_takes_the_nodes_an_answer_names() {
        let (reply, _answer) = oneshot::channel();
        let mut walk = Lookup::new(
            an_id(0),
            [1u8; 20],
            None,
            vec![(an_id(2), "10.0.0.2:6881".parse().expect("the address"))],
            reply,
        );
        let (address, _) = walk.next().expect("a question");
        walk.answer(
            address,
            &[(
                "nodes".into(),
                BencodeValue::Bytes(encode_nodes(&[(
                    an_id(1),
                    "10.0.0.1:6881".parse().expect("the address"),
                )])),
            )],
        );
        // The node that was named is nearer to the target, so it is asked next.
        let (next, _) = walk.next().expect("a question");
        assert_eq!(next.to_string(), "10.0.0.1:6881");
    }
}
