//! Whether an address may be reached at all.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;

/// The names that are never a public site.
const BLOCKED_HOSTS: &[&str] = &[
    "localhost",
    "metadata",
    "metadata.google.internal",
    "metadata.goog",
    "instance-data",
    "instance-data.ec2.internal",
    "metadata.azure.internal",
];

/// The suffixes that only mean something inside one network.
const LOCAL_SUFFIXES: &[&str] = &[
    ".localhost",
    ".local",
    ".internal",
    ".home.arpa",
    ".lan",
    ".test",
    ".invalid",
    ".example",
    ".onion",
];

/// Why an address cannot be crawled, or `None` when it can.
///
/// This checks the spelling of the host only; [`check_public`] also resolves
/// it, because a public name can point at a private address.
pub fn host_problem(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Some("The address has no host.".to_string());
    }
    if BLOCKED_HOSTS.contains(&host.as_str()) {
        return Some("That address is this computer, or a cloud metadata endpoint.".to_string());
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        if !addr_is_public(address) {
            return Some("That address is on this computer or a private network.".to_string());
        }
        return None;
    }
    if LOCAL_SUFFIXES.iter().any(|suffix| host.ends_with(suffix)) {
        return Some("That name only means something inside one network.".to_string());
    }
    // A name without a dot is looked up through the local search suffix, which
    // is how `intranet` becomes a machine next door rather than a site.
    if !host.contains('.') {
        return Some("That name only means something inside one network.".to_string());
    }
    None
}

/// Whether an address is one the public internet can reach.
pub fn addr_is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => ipv4_is_public(address),
        IpAddr::V6(address) => ipv6_is_public(address),
    }
}

/// The IPv4 ranges that are not a public destination.
///
/// This is every special-purpose block a request must not travel to: the
/// machine, the local network, the carrier's shared space, the documentation
/// ranges, and the blocks reserved for the future.
const BLOCKED_V4: &[(Ipv4Addr, u8)] = &[
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

fn ipv4_is_public(address: Ipv4Addr) -> bool {
    let value = u32::from(address);
    !BLOCKED_V4.iter().any(|(network, bits)| {
        let mask = u32::MAX << (32 - bits);
        value & mask == u32::from(*network) & mask
    })
}

fn ipv6_is_public(address: Ipv6Addr) -> bool {
    if let Some(embedded) = embedded_ipv4(address) {
        return ipv4_is_public(embedded);
    }
    let segments = address.segments();
    if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
        return false;
    }
    // fc00::/7 is the private space of IPv6, fe80::/10 the link-local one.
    if segments[0] & 0xfe00 == 0xfc00 || segments[0] & 0xffc0 == 0xfe80 {
        return false;
    }
    // 2001:db8::/32 and 100::/64 are documentation and discard space.
    if (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || (segments[0] == 0x0100 && segments[1..4].iter().all(|segment| *segment == 0))
    {
        return false;
    }
    true
}

/// The IPv4 address an IPv6 address carries, when it carries one.
///
/// Both spellings matter: `::ffff:127.0.0.1` is how a resolver often reports an
/// IPv4 address, and `::127.0.0.1` is the older compatible form.
fn embedded_ipv4(address: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return Some(mapped);
    }
    let segments = address.segments();
    // 64:ff9b::/96 is the NAT64 prefix: the last 32 bits are the IPv4
    // address the network would actually connect to.
    if segments[0] == 0x0064
        && segments[1] == 0xff9b
        && segments[2..6].iter().all(|segment| *segment == 0)
    {
        let high = segments[6];
        let low = segments[7];
        return Some(Ipv4Addr::new(
            (high >> 8) as u8,
            high as u8,
            (low >> 8) as u8,
            low as u8,
        ));
    }
    if segments[..6].iter().all(|segment| *segment == 0) {
        let last = segments[7];
        let before = segments[6];
        return Some(Ipv4Addr::new(
            (before >> 8) as u8,
            before as u8,
            (last >> 8) as u8,
            last as u8,
        ));
    }
    None
}

/// Whether an address is a web address at all. The host is neither spelled
/// out nor resolved; that is [`check_literal`]'s and [`check_public`]'s job.
pub fn check_web(url: &Url) -> Result<(), String> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("Only http:// and https:// addresses can be read.".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("An address with a user name or password is not followed.".to_string());
    }
    url.host_str()
        .ok_or_else(|| "The address has no host.".to_string())?;
    Ok(())
}

/// Whether the address itself is allowed. The host is not resolved.
pub fn check_literal(url: &Url) -> Result<(), String> {
    check_web(url)?;
    let host = url
        .host_str()
        .ok_or_else(|| "The address has no host.".to_string())?;
    if let Some(problem) = host_problem(host) {
        return Err(problem);
    }
    Ok(())
}

/// Whether a request may be sent to this address, after resolving its host.
///
/// Every page and every redirect goes through this. A name that resolves to
/// the machine, the local network or a metadata endpoint is refused before the
/// connection is made. A name that resolves to several addresses is refused if
/// any of them is private: a request must not be able to pick the private one.
pub async fn check_public(url: &Url) -> Result<(), String> {
    check_literal(url)?;
    let host = url
        .host_str()
        .ok_or_else(|| "The address has no host.".to_string())?;
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "The address of the site could not be looked up.".to_string())?;
    let mut seen = false;
    for address in addresses {
        seen = true;
        if !addr_is_public(address.ip()) {
            return Err("That name leads to a private network.".to_string());
        }
    }
    if !seen {
        return Err("The address of the site could not be looked up.".to_string());
    }
    Ok(())
}
