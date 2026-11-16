//! The routing table: the nodes this one knows, in the buckets the
//! specification describes.
//!
//! The table is a kademlia tree. Every node is put in the bucket of the
//! distance band it falls in, a bucket holds eight nodes, and a full bucket
//! is split in two when a node arrives that belongs nearer than the ones it
//! holds.

use std::cmp::Ordering;
use std::net::SocketAddr;

use crate::dht::krpc::NodeId;

/// How many nodes a bucket holds.
const BUCKET_SIZE: usize = 8;

/// One node of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// The id of the node.
    pub id: NodeId,
    /// Where the node is.
    pub address: SocketAddr,
}

/// Whether the first id is nearer to the target than the second is.
pub fn is_closer(first: &NodeId, second: &NodeId, target: &NodeId) -> bool {
    for index in 0..20 {
        let left = first[index] ^ target[index];
        let right = second[index] ^ target[index];
        if left != right {
            return left < right;
        }
    }
    false
}

/// The distance band a node falls in: zero is the nearest to this node, one
/// hundred and fifty-nine the farthest.
fn band(local: &NodeId, id: &NodeId) -> u8 {
    for index in 0..20 {
        if local[index] != id[index] {
            let difference = local[index] ^ id[index];
            // The band counts from the lowest bit, so the byte that differs
            // gives the band and the bit inside it gives the rest.
            return (index as u8) * 8 + (7 - difference.leading_zeros() as u8);
        }
    }
    0
}

/// One bucket of the table.
#[derive(Clone, Debug)]
struct Bucket {
    /// The first band this bucket holds.
    low: u8,
    /// The last band this bucket holds.
    high: u8,
    /// The nodes of the bucket.
    nodes: Vec<Node>,
}

/// The nodes this one knows.
#[derive(Clone, Debug)]
pub struct RoutingTable {
    /// The id of this node.
    local: NodeId,
    /// The buckets, from the nearest to the farthest.
    buckets: Vec<Bucket>,
}

impl RoutingTable {
    /// A table that knows nothing but this node.
    pub fn new(local: NodeId) -> RoutingTable {
        RoutingTable {
            local,
            buckets: vec![Bucket {
                low: 0,
                high: 159,
                nodes: Vec::new(),
            }],
        }
    }

    /// How many nodes the table holds.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.buckets.iter().map(|bucket| bucket.nodes.len()).sum()
    }

    /// Whether the table holds no node at all.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Put a node into the table. It says whether the node was taken. A node
    /// that is already there keeps its place; a new address for it is taken,
    /// the same one is not news.
    pub fn add(&mut self, id: NodeId, address: SocketAddr) -> bool {
        if id == self.local {
            return false;
        }
        for bucket in &mut self.buckets {
            if let Some(node) = bucket.nodes.iter_mut().find(|node| node.id == id) {
                if node.address == address {
                    return false;
                }
                node.address = address;
                return true;
            }
        }
        let band = band(&self.local, &id);
        // The bucket is found and, when it is full, split. Every split halves
        // the range of bands, so the walk below ends after eight rounds.
        for _ in 0..8 {
            let index = match self.bucket_of(band) {
                Some(index) => index,
                None => return false,
            };
            if self.buckets[index].nodes.len() < BUCKET_SIZE {
                self.buckets[index].nodes.push(Node { id, address });
                let local = self.local;
                self.buckets[index]
                    .nodes
                    .sort_by(|first, second| compare(&local, &first.id, &second.id));
                return true;
            }
            if !self.split(index) {
                // The bucket cannot be split any further, so the node is left
                // out: the table keeps the nodes it knows.
                return false;
            }
        }
        false
    }

    /// The nodes nearest to a target, the nearest first.
    pub fn closest(&self, target: &NodeId, count: usize) -> Vec<Node> {
        let mut nodes: Vec<Node> = self
            .buckets
            .iter()
            .flat_map(|bucket| bucket.nodes.iter().cloned())
            .collect();
        nodes.sort_by(|first, second| {
            if is_closer(&first.id, &second.id, target) {
                Ordering::Less
            } else if is_closer(&second.id, &first.id, target) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });
        nodes.truncate(count);
        nodes
    }

    /// The bucket a band falls in.
    fn bucket_of(&self, band: u8) -> Option<usize> {
        self.buckets
            .iter()
            .position(|bucket| band >= bucket.low && band <= bucket.high)
    }

    /// Split a bucket in two. It says whether the bucket could be split.
    fn split(&mut self, index: usize) -> bool {
        let (low, high) = (self.buckets[index].low, self.buckets[index].high);
        if low >= high {
            return false;
        }
        let middle = low + (high - low) / 2;
        let nodes = std::mem::take(&mut self.buckets[index].nodes);
        let mut near = Vec::new();
        let mut far = Vec::new();
        for node in nodes {
            if band(&self.local, &node.id) <= middle {
                near.push(node);
            } else {
                far.push(node);
            }
        }
        self.buckets[index].nodes = near;
        self.buckets[index].high = middle;
        self.buckets.insert(
            index + 1,
            Bucket {
                low: middle + 1,
                high,
                nodes: far,
            },
        );
        true
    }
}

/// The order of two nodes by their distance to a local id.
fn compare(local: &NodeId, first: &NodeId, second: &NodeId) -> Ordering {
    for index in 0..20 {
        let left = first[index] ^ local[index];
        let right = second[index] ^ local[index];
        if left != right {
            return left.cmp(&right);
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_id(seed: u8) -> NodeId {
        [seed; 20]
    }

    fn an_address(seed: u8) -> SocketAddr {
        SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, seed, 1)),
            6881,
        )
    }

    #[test]
    fn the_table_keeps_the_nodes_nearest_to_a_target_first() {
        let mut table = RoutingTable::new(an_id(0));
        // The target is the node that differs from this one in the last byte.
        let target = an_id(1);
        table.add(an_id(9), an_address(9));
        table.add(target, an_address(1));
        table.add(an_id(5), an_address(5));

        let closest = table.closest(&target, 2);
        assert_eq!(closest[0].id, target);
        // Of the other two, five is nearer to one than nine is: the distance
        // is the exclusive-or, byte by byte.
        assert_eq!(closest[1].id, an_id(5));
    }

    #[test]
    fn a_bucket_holds_eight_nodes_and_then_splits() {
        let mut table = RoutingTable::new(an_id(0));
        // Every node of a different band: the first bucket fills and splits.
        for seed in 1..=9u8 {
            let mut id = an_id(0);
            id[19] = seed * 16;
            assert!(table.add(id, an_address(seed)));
        }
        assert!(table.len() >= 8);
        // The table is split into more than one bucket now.
        assert!(table.buckets.len() > 1);
    }

    #[test]
    fn a_node_is_not_taken_twice_and_this_node_is_not_a_node() {
        let mut table = RoutingTable::new(an_id(0));
        assert!(table.add(an_id(3), an_address(3)));
        assert!(!table.add(an_id(3), an_address(3)));
        assert!(!table.add(an_id(0), an_address(0)));
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }
}
