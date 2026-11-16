//! NAT-PMP (RFC 6886): the port mapping for a router that speaks no UPnP.
//!
//! The request is twelve bytes to the gateway's port 5351: map this internal
//! TCP port to an external one, for this many seconds. The answer names the
//! external port the router chose and the lease it gave. A lifetime of zero
//! gives the mapping back. The gateway is the machine's own default route —
//! the RFC speaks to the gateway directly, not to a multicast group.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::TorrentError;

/// The port NAT-PMP is spoken on.
pub const PORT: u16 = 5351;

/// The request that asks for a TCP mapping. A lease of zero deletes it.
pub fn map_request(internal: u16, external: u16, lease: u32) -> [u8; 12] {
    let mut bytes = [0u8; 12];
    bytes[0] = 0; // version
    bytes[1] = 1; // opcode: map TCP
    bytes[2..4].copy_from_slice(&internal.to_be_bytes());
    bytes[4..6].copy_from_slice(&external.to_be_bytes());
    bytes[8..12].copy_from_slice(&lease.to_be_bytes());
    bytes
}

/// The answer, read: the external port and the lease, or the refusal.
pub fn parse_map_response(bytes: &[u8]) -> Result<(u16, u32), TorrentError> {
    if bytes.len() < 16 {
        return Err(TorrentError::Network(
            "the gateway answered with too little".into(),
        ));
    }
    if bytes[0] != 0 {
        return Err(TorrentError::Network(
            "the gateway speaks another version of NAT-PMP".into(),
        ));
    }
    if bytes[1] != 129 {
        return Err(TorrentError::Network(
            "the gateway answered another question".into(),
        ));
    }
    let result = u16::from_be_bytes([bytes[2], bytes[3]]);
    if result != 0 {
        return Err(TorrentError::Network(format!(
            "the gateway refused the port mapping (code {result})"
        )));
    }
    let external = u16::from_be_bytes([bytes[10], bytes[11]]);
    let lease = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    Ok((external, lease))
}

/// The default gateway, out of the routing table the system prints.
pub fn gateway_from_route(text: &str) -> Option<Ipv4Addr> {
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // Windows: "0.0.0.0  0.0.0.0  192.168.1.1  ..." under Active Routes.
        if fields.len() >= 3 && fields[0] == "0.0.0.0" && fields[1] == "0.0.0.0" {
            if let Ok(gateway) = fields[2].parse() {
                return Some(gateway);
            }
        }
        // Linux and macOS: "default via 192.168.1.1 dev ...".
        if fields.first() == Some(&"default") {
            if let Some(at) = fields.iter().position(|field| *field == "via") {
                if let Some(gateway) = fields.get(at + 1).and_then(|text| text.parse().ok()) {
                    return Some(gateway);
                }
            }
        }
    }
    None
}

/// The machine's default gateway, as the routing table names it.
pub async fn default_gateway() -> Option<Ipv4Addr> {
    let output = if cfg!(windows) {
        tokio::process::Command::new("route")
            .args(["print", "0.0.0.0"])
            .output()
            .await
            .ok()?
    } else {
        tokio::process::Command::new("ip")
            .args(["route", "show", "default"])
            .output()
            .await
            .ok()?
    };
    gateway_from_route(&String::from_utf8_lossy(&output.stdout))
}

/// Ask the gateway for a mapping. `Ok(None)` is a gateway that did not
/// answer — no NAT-PMP here — and a refusal is the gateway's own.
pub async fn map(
    gateway: Ipv4Addr,
    port: u16,
    lease: u32,
) -> Result<Option<(u16, u32)>, TorrentError> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let target = SocketAddr::new(std::net::IpAddr::V4(gateway), PORT);
    socket
        .send_to(&map_request(port, port, lease), target)
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let mut buffer = [0u8; 64];
    let received = tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut buffer)).await;
    let Ok(Ok((length, _))) = received else {
        return Ok(None);
    };
    let (external, lease) = parse_map_response(&buffer[..length])?;
    Ok(Some((external, lease)))
}

/// Give the mapping back: the same request with a lifetime of zero.
pub async fn unmap(gateway: Ipv4Addr, port: u16, external: u16) -> Result<(), TorrentError> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let target = SocketAddr::new(std::net::IpAddr::V4(gateway), PORT);
    socket
        .send_to(&map_request(port, external, 0), target)
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_is_the_twelve_bytes_of_the_rfc() {
        let bytes = map_request(6881, 6881, 3600);
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[1], 1);
        assert_eq!(&bytes[2..4], &6881u16.to_be_bytes());
        assert_eq!(&bytes[4..6], &6881u16.to_be_bytes());
        assert_eq!(&bytes[8..12], &3600u32.to_be_bytes());
        // A delete is the same request with no lifetime.
        let delete = map_request(6881, 6881, 0);
        assert_eq!(&delete[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn an_answer_names_the_port_and_the_lease() {
        let mut bytes = [0u8; 16];
        bytes[0] = 0;
        bytes[1] = 129;
        bytes[8..10].copy_from_slice(&6881u16.to_be_bytes());
        bytes[10..12].copy_from_slice(&4242u16.to_be_bytes());
        bytes[12..16].copy_from_slice(&3600u32.to_be_bytes());
        assert_eq!(parse_map_response(&bytes).unwrap(), (4242, 3600));
    }

    #[test]
    fn a_refusal_is_the_gateways_own() {
        let mut bytes = [0u8; 16];
        bytes[1] = 129;
        bytes[2..4].copy_from_slice(&2u16.to_be_bytes()); // not authorized
        let error = parse_map_response(&bytes).unwrap_err();
        assert!(error.to_string().contains("code 2"));
        assert!(parse_map_response(&[0u8; 4]).is_err());
        let mut wrong = [0u8; 16];
        wrong[1] = 128;
        assert!(parse_map_response(&wrong).is_err());
    }

    #[test]
    fn the_gateway_is_read_from_the_routing_table() {
        let windows = "===========================================================================\n\
Interface List\n 10...00 11 22 33 44 55 ...... Ethernet\n\
===========================================================================\n\
\nActive Routes:\n\
Network Destination        Netmask          Gateway       Interface  Metric\n\
          0.0.0.0          0.0.0.0      192.168.1.1     192.168.1.10     25\n\
        127.0.0.0        255.0.0.0         On-link         127.0.0.1    331\n";
        assert_eq!(
            gateway_from_route(windows),
            Some("192.168.1.1".parse().unwrap())
        );
        let linux = "default via 10.0.0.1 dev eth0 proto dhcp metric 100\n10.0.0.0/24 dev eth0 scope link\n";
        assert_eq!(
            gateway_from_route(linux),
            Some("10.0.0.1".parse().unwrap())
        );
        assert_eq!(gateway_from_route("no routes here"), None);
    }
}