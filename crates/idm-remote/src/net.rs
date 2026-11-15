//! The rule that keeps the remote inside the house.
//!
//! The remote is only useful on the reader's own network, so two checks are
//! applied to every request: the address the connection came from has to be a
//! local one, and the `Host` header has to name this machine. The second one
//! matters because a name that resolves to a local address can be pointed
//! somewhere else, and a request that arrives with a foreign name is a
//! rebinding attempt, not a phone on the sofa.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Whether the address the connection came from is on the local network.
///
/// Loopback, the private ranges of RFC 1918, the link-local ranges and the
/// IPv6 unique-local range are local. Everything else is refused, so a port
/// that was forwarded to the outside world does not become a door.
pub fn is_local_address(addr: &SocketAddr) -> bool {
    is_local_ip(addr.ip())
}

/// Whether the `Host` header names this machine.
///
/// `localhost` and any local address are accepted, with the port the server
/// listens on. A name is refused: the phone reaches the server by its
/// address, and a name can be pointed anywhere.
pub fn host_header_is_local(host: &str, port: u16) -> bool {
    let host = host.trim();
    if host.is_empty() {
        return false;
    }

    let (name, given) = split_host(host);
    if let Some(given) = given {
        if given != port {
            return false;
        }
    }

    let name = name.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if name == "localhost" {
        return true;
    }
    match name.parse::<IpAddr>() {
        Ok(ip) => is_local_ip(ip),
        Err(_) => false,
    }
}

/// The name of a `Host` header and the port it names, if it names one.
pub(crate) fn split_host(host: &str) -> (&str, Option<u16>) {
    // An IPv6 address is written in brackets, so its colons are not a port.
    if let Some(rest) = host.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return (host, None);
        };
        let name = &rest[..end];
        let tail = &rest[end + 1..];
        let port = tail.strip_prefix(':').and_then(|port| port.parse().ok());
        return (name, port);
    }

    match host.rsplit_once(':') {
        Some((name, port)) => match port.parse::<u16>() {
            Ok(port) => (name, Some(port)),
            Err(_) => (host, None),
        },
        None => (host, None),
    }
}

/// Whether the address is one of the local ranges.
fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_local_v4(v4),
        IpAddr::V6(v6) => is_local_v6(v6),
    }
}

/// Loopback, the private ranges and link-local.
fn is_local_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private() || ip.is_link_local()
}

/// Loopback, the unique-local range and link-local.
fn is_local_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    // A connection from an IPv4 address that reached us over IPv6 arrives
    // wrapped, and the range that matters is the one inside.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_local_v4(v4);
    }
    let first = ip.segments()[0];
    (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(address: &str) -> SocketAddr {
        SocketAddr::new(address.parse().unwrap(), 1234)
    }

    fn v6(address: &str) -> SocketAddr {
        SocketAddr::new(address.parse().unwrap(), 1234)
    }

    #[test]
    fn the_private_ranges_are_local() {
        assert!(is_local_address(&v4("127.0.0.1")));
        assert!(is_local_address(&v4("192.168.1.10")));
        assert!(is_local_address(&v4("10.0.0.5")));
        assert!(is_local_address(&v4("172.16.0.1")));
        assert!(is_local_address(&v4("172.31.255.254")));
    }

    #[test]
    fn the_outside_world_is_not_local() {
        assert!(!is_local_address(&v4("8.8.8.8")));
        assert!(!is_local_address(&v4("203.0.113.5")));
        assert!(!is_local_address(&v4("172.32.0.1")));
        assert!(!is_local_address(&v4("0.0.0.0")));
    }

    #[test]
    fn the_ipv6_local_ranges_are_local() {
        assert!(is_local_address(&v6("::1")));
        assert!(is_local_address(&v6("fd00::1")));
        assert!(is_local_address(&v6("fe80::1")));
        assert!(!is_local_address(&v6("2001:db8::1")));
    }

    #[test]
    fn the_host_header_names_this_machine_or_nothing() {
        assert!(host_header_is_local("127.0.0.1:8080", 8080));
        assert!(host_header_is_local("localhost:8080", 8080));
        assert!(host_header_is_local("LOCALHOST:8080", 8080));
        assert!(host_header_is_local("192.168.1.10:8080", 8080));
        assert!(host_header_is_local("127.0.0.1", 8080));
    }

    #[test]
    fn a_foreign_host_header_is_refused() {
        assert!(!host_header_is_local("evil.example.com", 8080));
        assert!(!host_header_is_local("evil.example.com:8080", 8080));
        assert!(!host_header_is_local("8.8.8.8:8080", 8080));
        assert!(!host_header_is_local("127.0.0.1:9999", 8080));
        assert!(!host_header_is_local("", 8080));
    }

    #[test]
    fn an_ipv6_host_header_is_read() {
        assert!(host_header_is_local("[::1]:8080", 8080));
        assert!(!host_header_is_local("[2001:db8::1]:8080", 8080));
    }
}
