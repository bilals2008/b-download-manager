//! Where a server is, and how far away it is.
//!
//! The progress window tells the reader which country answers, and how far
//! the connection is, so a slow download can be told apart from a slow
//! server.
//!
//! The country comes from the table the crate carries (DB-IP's IP to Country
//! Lite, looked up on this computer, never asked of anyone); the round trip
//! is measured by opening a connection to the server.

mod countries;

pub use countries::{country, prepare, Country, ATTRIBUTION, ATTRIBUTION_LICENCE, ATTRIBUTION_URL};

use std::io;
use std::time::Instant;

/// The answer of a measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    /// The address the server answered from.
    pub address: String,
    /// The time the round trip took, in milliseconds.
    pub rtt_ms: u64,
}

/// How far away a server is, in the words the reference manager uses: under
/// 60 ms near (the same country or next door), under 150 ms medium (the same
/// continent), else far.
pub fn distance(rtt_ms: u64) -> &'static str {
    if rtt_ms < 60 {
        "near"
    } else if rtt_ms < 150 {
        "medium"
    } else {
        "far"
    }
}

/// The round trip to a server, measured by opening a connection to it.
///
/// The name is resolved first, so the time is the server's answer and not the
/// name server's: what a slow download feels is the distance, not the
/// lookup. The address the server answered from is kept, because one name can
/// be served from several places, and it is the place that answers that
/// matters.
pub async fn measure(host: &str, port: u16) -> io::Result<Reach> {
    let mut last = None;
    for address in tokio::net::lookup_host((host, port)).await? {
        let start = Instant::now();
        match tokio::net::TcpStream::connect(address).await {
            Ok(stream) => {
                let peer = stream.peer_addr()?;
                return Ok(Reach {
                    address: peer.ip().to_string(),
                    rtt_ms: start.elapsed().as_millis() as u64,
                });
            }
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            "the server has no address to answer from",
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_trip_is_near_medium_or_far() {
        assert_eq!(distance(0), "near");
        assert_eq!(distance(59), "near");
        assert_eq!(distance(60), "medium");
        assert_eq!(distance(149), "medium");
        assert_eq!(distance(150), "far");
        assert_eq!(distance(900), "far");
    }

    #[tokio::test]
    async fn a_server_answers_with_its_own_address() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("the port is free");
        let port = listener.local_addr().expect("the address is known").port();
        let reach = measure("127.0.0.1", port)
            .await
            .expect("the server answers");
        assert_eq!(reach.address, "127.0.0.1");
        assert_eq!(distance(reach.rtt_ms), "near");
    }

    #[tokio::test]
    async fn a_server_that_does_not_answer_is_an_error() {
        // A port nothing listens on: the connection is refused at once.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("the port is free");
        let port = listener.local_addr().expect("the address is known").port();
        drop(listener);
        assert!(measure("127.0.0.1", port).await.is_err());
    }
}
