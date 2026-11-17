//! The values a node holds, and the tokens that guard them.
//!
//! The specification says the token a node hands out is derived from the
//! address of the peer it was handed to, so a peer can only announce itself
//! where it was told it could. This node keeps that rule: the token is the
//! hash of its own secret and the address of the peer.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use crate::util::sha1_bytes;

/// How long a value is kept before it is dropped.
const LIFETIME: Duration = Duration::from_secs(2 * 60 * 60);

/// How often the secret the tokens are made of changes. The specification
/// says a token should not outlive a few minutes; the one before is still
/// taken back, so an announce that was given a token just before the change
/// is not refused for it.
const ROTATE: Duration = Duration::from_secs(5 * 60);

/// The peers that announced an info hash.
#[derive(Clone, Debug)]
struct Value {
    /// The peers that announced themselves.
    peers: Vec<SocketAddr>,
    /// When the value goes out of date.
    expires: Instant,
}

/// The values this node holds.
#[derive(Clone, Debug)]
pub struct Store {
    /// The secret of this node, which the tokens are made of.
    secret: [u8; 20],
    /// The secret before the last change, whose tokens are still taken back.
    previous: Option<[u8; 20]>,
    /// When the secret changed last.
    rotated: Instant,
    /// The values, by info hash.
    values: HashMap<[u8; 20], Value>,
}

impl Store {
    /// A store that holds nothing yet.
    pub fn new(secret: [u8; 20]) -> Store {
        Store {
            secret,
            previous: None,
            rotated: Instant::now(),
            values: HashMap::new(),
        }
    }

    /// The token this node gives a peer, and the one it expects back.
    pub fn token(&self, address: IpAddr) -> Vec<u8> {
        Self::token_of(&self.secret, address)
    }

    /// Whether a token is one this node gave to that address, now or before
    /// the last change of the secret.
    pub fn accepts(&self, address: IpAddr, token: &[u8]) -> bool {
        self.token(address) == token
            || self
                .previous
                .is_some_and(|previous| Self::token_of(&previous, address) == token)
    }

    /// The token a secret makes for an address.
    fn token_of(secret: &[u8; 20], address: IpAddr) -> Vec<u8> {
        let mut data = secret.to_vec();
        match address {
            IpAddr::V4(v4) => data.extend_from_slice(&v4.octets()),
            IpAddr::V6(v6) => data.extend_from_slice(&v6.octets()),
        }
        sha1_bytes(&data).to_vec()
    }

    /// Remember that a peer has an info hash.
    pub fn announce(&mut self, info_hash: [u8; 20], peer: SocketAddr) {
        let entry = self.values.entry(info_hash).or_insert_with(|| Value {
            peers: Vec::new(),
            expires: Instant::now() + LIFETIME,
        });
        entry.expires = Instant::now() + LIFETIME;
        if !entry.peers.contains(&peer) {
            entry.peers.push(peer);
        }
    }

    /// The peers that announced an info hash.
    pub fn peers(&self, info_hash: &[u8; 20]) -> Vec<SocketAddr> {
        self.values
            .get(info_hash)
            .map(|value| value.peers.clone())
            .unwrap_or_default()
    }

    /// Drop the values that went out of date, and change the secret when it
    /// has served its minutes.
    pub fn expire(&mut self) {
        let now = Instant::now();
        self.values.retain(|_, value| value.expires > now);
        if now.duration_since(self.rotated) >= ROTATE {
            self.previous = Some(self.secret);
            self.secret = sha1_bytes(&[self.secret.as_slice(), b"rotate".as_slice()].concat());
            self.rotated = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_given_to_an_address_and_taken_back_from_it() {
        let store = Store::new([7; 20]);
        let address: IpAddr = "10.0.0.1".parse().expect("the address");
        let token = store.token(address);
        assert_eq!(token.len(), 20);
        assert!(store.accepts(address, &token));
        assert!(!store.accepts("10.0.0.2".parse().expect("the address"), &token));
        assert!(!store.accepts(address, b"not the token"));
    }

    #[test]
    fn the_peers_of_an_info_hash_are_remembered() {
        let mut store = Store::new([7; 20]);
        let hash = [1u8; 20];
        let peer: SocketAddr = "10.0.0.1:6881".parse().expect("the address");
        assert!(store.peers(&hash).is_empty());
        store.announce(hash, peer);
        store.announce(hash, peer);
        assert_eq!(store.peers(&hash), vec![peer]);
        assert!(store.peers(&[2u8; 20]).is_empty());
    }
}
