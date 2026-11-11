//! The resolver the engine asks, and the rules about when it may be asked.
//!
//! The reader chooses a resolver: the system's, a plain DNS server over UDP,
//! DNS over TLS or DNS over HTTPS. Two rules hold whatever the choice is,
//! and both exist because a public resolver cannot know what only the local
//! network knows:
//!
//! - the names in the hosts file and the local names (`localhost`, a
//!   single-label name, `.local`, `.lan`, `.home.arpa`, `.internal`) always
//!   stay with the system's resolver. An intranet address would simply not
//!   resolve otherwise, and a download of a file on the office server is
//!   exactly the case the reader switched the DNS for in the first place.
//! - when the chosen resolver cannot be reached, the system's may answer
//!   instead, but only if the reader allowed it. A name the chosen resolver
//!   says does not exist never falls back: that would undo a filtering
//!   resolver, which is the reason people choose one.
//!
//! Nothing here asks a name: this is the choice and the rules around it, not
//! a DNS client.

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

/// The kind of resolver, as the settings window offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolverKind {
    /// The resolver of the operating system: its cache, its hosts file, and
    /// Windows' own encrypted DNS when it is set up there.
    System,
    /// A plain DNS server, asked over UDP.
    Plain,
    /// DNS over TLS (RFC 7858).
    OverTls,
    /// DNS over HTTPS (RFC 8484).
    OverHttps,
}

impl ResolverKind {
    /// The name of the kind, as the settings window shows it.
    pub fn name(self) -> &'static str {
        match self {
            ResolverKind::System => "System",
            ResolverKind::Plain => "Plain DNS",
            ResolverKind::OverTls => "DNS over TLS",
            ResolverKind::OverHttps => "DNS over HTTPS",
        }
    }
}

/// The resolver the engine asks: its kind, the name it is shown under, its
/// address and its port.
///
/// The three are one type because the settings window shows them as one
/// choice: a resolver without an address is only the system's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolver {
    /// How the name is resolved.
    pub kind: ResolverKind,
    /// The name the settings window shows ("Cloudflare", "NextDNS").
    pub name: String,
    /// The address of the server: an IP or a host for plain DNS and DNS over
    /// TLS, a `https://` address for DNS over HTTPS.
    pub address: String,
    /// The port of the server.
    pub port: u16,
}

impl Default for Resolver {
    fn default() -> Self {
        Resolver::system()
    }
}

impl Resolver {
    /// The system's resolver.
    pub fn system() -> Self {
        Resolver {
            kind: ResolverKind::System,
            name: "System".to_owned(),
            address: String::new(),
            port: 0,
        }
    }

    /// A plain DNS server, asked over UDP.
    pub fn plain(name: impl Into<String>, address: impl Into<String>, port: u16) -> Self {
        Resolver {
            kind: ResolverKind::Plain,
            name: name.into(),
            address: address.into(),
            port,
        }
    }

    /// A DNS server that speaks DNS over TLS.
    pub fn over_tls(name: impl Into<String>, address: impl Into<String>, port: u16) -> Self {
        Resolver {
            kind: ResolverKind::OverTls,
            name: name.into(),
            address: address.into(),
            port,
        }
    }

    /// A DNS server that speaks DNS over HTTPS: the address is a URL.
    pub fn over_https(name: impl Into<String>, url: impl Into<String>) -> Self {
        Resolver {
            kind: ResolverKind::OverHttps,
            name: name.into(),
            address: url.into(),
            port: 443,
        }
    }

    /// Whether this is the system's resolver.
    pub fn is_system(&self) -> bool {
        self.kind == ResolverKind::System
    }

    /// The name the messages use ("Cloudflare (DNS over TLS)").
    pub fn label(&self) -> String {
        match self.kind {
            ResolverKind::System => self.name.clone(),
            _ => format!("{} ({})", self.name, self.kind.name()),
        }
    }
}

/// A resolver the settings window offers by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// The name the settings file stores ("cloudflare").
    pub id: &'static str,
    /// The name the reader reads ("Cloudflare").
    pub name: &'static str,
    /// How it is asked.
    pub kind: ResolverKind,
    /// The address of the server.
    pub address: &'static str,
    /// The port of the server.
    pub port: u16,
}

/// The resolvers the settings window offers by name.
///
/// They are DNS over TLS, because a plain server cannot be told apart from a
/// name and because the encrypted one survives a network that watches.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "cloudflare",
        name: "Cloudflare",
        kind: ResolverKind::OverTls,
        address: "one.one.one.one",
        port: 853,
    },
    Preset {
        id: "google",
        name: "Google",
        kind: ResolverKind::OverTls,
        address: "dns.google",
        port: 853,
    },
    Preset {
        id: "quad9",
        name: "Quad9",
        kind: ResolverKind::OverTls,
        address: "dns.quad9.io",
        port: 853,
    },
    Preset {
        id: "adguard",
        name: "AdGuard",
        kind: ResolverKind::OverTls,
        address: "dns.adguard-dns.com",
        port: 853,
    },
    Preset {
        id: "nextdns",
        name: "NextDNS",
        kind: ResolverKind::OverTls,
        address: "dns.nextdns.io",
        port: 853,
    },
    Preset {
        id: "cloudflare-plain",
        name: "Cloudflare (plain DNS)",
        kind: ResolverKind::Plain,
        address: "1.1.1.1",
        port: 53,
    },
];

impl Preset {
    /// The preset with that id.
    pub fn by_id(id: &str) -> Option<&'static Preset> {
        PRESETS.iter().find(|p| p.id == id)
    }

    /// The resolver for this preset. `profile` is a NextDNS configuration id
    /// (six hex digits), which goes into the name its certificate shows;
    /// empty for NextDNS's default.
    pub fn resolver(&self, profile: &str) -> Resolver {
        let profile = profile.trim();
        let address = if self.id == "nextdns" && !profile.is_empty() {
            format!("{profile}.{}", self.address)
        } else {
            self.address.to_owned()
        };
        Resolver {
            kind: self.kind,
            name: self.name.to_owned(),
            address,
            port: self.port,
        }
    }
}

/// Whether `id` is a NextDNS configuration id: six hex digits, or none.
pub fn valid_nextdns_profile(id: &str) -> bool {
    let id = id.trim();
    id.is_empty() || (id.len() == 6 && id.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The resolver the engine uses, and whether the system's may answer when
/// the chosen one cannot be reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResolverChoice {
    /// The resolver that answers.
    pub resolver: Resolver,
    /// Whether the system's resolver may answer instead when the chosen one
    /// cannot be reached.
    pub fallback: bool,
}

impl ResolverChoice {
    /// The system's resolver, without a fallback.
    pub fn system() -> Self {
        ResolverChoice {
            resolver: Resolver::system(),
            fallback: false,
        }
    }

    /// Whether the engine asks the system's resolver.
    pub fn is_system(&self) -> bool {
        self.resolver.is_system()
    }
}

/// Who answers a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    /// The chosen resolver answers.
    Chosen,
    /// The system's resolver answers.
    System,
}

/// The names the reader pinned to addresses by hand (the hosts file).
///
/// They are answered from the file itself and are never sent to a chosen
/// resolver: a public resolver knows nothing about them and would answer
/// "no such name".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hosts {
    /// The names, lowercased, and their addresses.
    entries: BTreeMap<String, Vec<IpAddr>>,
}

impl Hosts {
    /// The addresses pinned to `host`, if any.
    pub fn get(&self, host: &str) -> Option<&[IpAddr]> {
        self.entries.get(&host.trim().to_ascii_lowercase()).map(Vec::as_slice)
    }

    /// Pins `host` to `addresses`, replacing what was there.
    pub fn set(&mut self, host: &str, addresses: Vec<IpAddr>) {
        self.entries
            .insert(host.trim().to_ascii_lowercase(), addresses);
    }

    /// Forgets `host`.
    pub fn remove(&mut self, host: &str) {
        self.entries.remove(&host.trim().to_ascii_lowercase());
    }

    /// How many names are pinned.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is pinned.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Whether `host` is a name only the system's resolver can answer: a
/// single-label name, or one of the suffixes a local network uses.
pub fn is_local_name(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if !host.contains('.') {
        return true;
    }
    ["localhost", "local", "lan", "home.arpa", "internal"]
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

/// Who answers `host`: the chosen resolver, or the system's.
///
/// The system's resolver answers the names in the hosts file and the local
/// names, whatever was chosen: only it knows them.
pub fn who_answers(choice: &ResolverChoice, hosts: &Hosts, host: &str) -> Who {
    if choice.is_system() || hosts.get(host).is_some() || is_local_name(host) {
        Who::System
    } else {
        Who::Chosen
    }
}

/// Why a lookup through the chosen resolver failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupFailure {
    /// The resolver answered: there is no such name.
    NoSuchName,
    /// The resolver could not be asked, or did not answer.
    Unreachable,
}

/// Whether the system's resolver may answer instead after a failure.
///
/// Only a failure of the resolver itself allows the fallback. A name it says
/// does not exist must stay that way: falling back would undo a filtering
/// resolver, which is why the reader chose one.
pub fn may_fall_back(choice: &ResolverChoice, failure: LookupFailure) -> bool {
    choice.fallback && failure == LookupFailure::Unreachable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_resolver_is_the_default() {
        assert!(ResolverChoice::default().is_system());
        assert_eq!(Resolver::default(), Resolver::system());
    }

    #[test]
    fn a_resolver_carries_its_name_address_and_port() {
        let r = Resolver::over_tls("Cloudflare", "one.one.one.one", 853);
        assert_eq!((r.kind, r.name.as_str(), r.address.as_str(), r.port), (
            ResolverKind::OverTls,
            "Cloudflare",
            "one.one.one.one",
            853
        ));
        assert_eq!(r.label(), "Cloudflare (DNS over TLS)");
        assert_eq!(Resolver::over_https("DoH", "https://d.test/q").port, 443);
    }

    #[test]
    fn nextdns_takes_its_configuration_id() {
        let n = Preset::by_id("nextdns").expect("NextDNS is offered");
        assert_eq!(n.resolver("").address, "dns.nextdns.io");
        assert_eq!(n.resolver("abc123").address, "abc123.dns.nextdns.io");
        assert!(valid_nextdns_profile("") && valid_nextdns_profile("a1b2c3"));
        assert!(!valid_nextdns_profile("xyz") && !valid_nextdns_profile("a1b2c3d"));
    }

    #[test]
    fn the_hosts_file_and_the_local_names_stay_with_the_system() {
        let choice = ResolverChoice {
            resolver: Resolver::over_tls("Cloudflare", "one.one.one.one", 853),
            fallback: true,
        };
        let mut hosts = Hosts::default();
        hosts.set("files.corp", vec!["10.0.0.5".parse().unwrap()]);

        for name in [
            "localhost",
            "nas",
            "printer.local",
            "a.b.lan",
            "x.home.arpa",
            "svc.internal",
            "app.localhost",
            "files.corp",
        ] {
            assert_eq!(who_answers(&choice, &hosts, name), Who::System, "{name}");
        }
        for name in ["github.com", "example.org", "local.example.com"] {
            assert_eq!(who_answers(&choice, &hosts, name), Who::Chosen, "{name}");
        }
    }

    #[test]
    fn a_name_that_does_not_exist_never_falls_back() {
        let choice = ResolverChoice {
            resolver: Resolver::over_tls("AdGuard", "dns.adguard-dns.com", 853),
            fallback: true,
        };
        assert!(may_fall_back(&choice, LookupFailure::Unreachable));
        assert!(!may_fall_back(&choice, LookupFailure::NoSuchName));
        let strict = ResolverChoice {
            fallback: false,
            ..choice
        };
        assert!(!may_fall_back(&strict, LookupFailure::Unreachable));
    }
}
