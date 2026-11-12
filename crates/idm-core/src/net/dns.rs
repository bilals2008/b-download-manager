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
//! a DNS client. The asking lives in [`super::resolver`].

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

    /// Every pinned name and its addresses, lowercased.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &[IpAddr])> {
        self.entries
            .iter()
            .map(|(name, ips)| (name.as_str(), ips.as_slice()))
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

/// How the reader wants names resolved, as the settings file stores it.
///
/// The stored values are exactly what the settings window shows: a mode, one
/// endpoint per mode, and whether the system's resolver may answer when the
/// chosen one cannot be reached. The strings stay strings in the file so a
/// hand-edited value this build does not know reads as the system's resolver
/// instead of making the whole file unreadable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DnsSettings {
    /// "system", "doh", "dot" or "nextdns".
    pub mode: String,
    /// The DNS over HTTPS endpoint: `https://host[:port]/path`, no credentials.
    pub doh_url: String,
    /// The hostname asked over TLS (RFC 7858), at port 853.
    pub dot_host: String,
    /// The NextDNS configuration id (six hex digits), empty for its default.
    pub nextdns_profile: String,
    /// Whether the system's resolver may answer when the chosen one cannot be
    /// reached. Off by default: falling back without the reader asking is a
    /// privacy leak.
    pub fallback: bool,
}

impl Default for DnsSettings {
    fn default() -> Self {
        Self {
            mode: DnsMode::System.stored().to_string(),
            doh_url: String::new(),
            dot_host: String::new(),
            nextdns_profile: String::new(),
            fallback: false,
        }
    }
}

/// The four modes the settings window offers, as stored in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsMode {
    System,
    Doh,
    Dot,
    Nextdns,
}

impl DnsMode {
    /// The mode the stored string names; anything else is the system's.
    pub fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "doh" => DnsMode::Doh,
            "dot" => DnsMode::Dot,
            "nextdns" => DnsMode::Nextdns,
            _ => DnsMode::System,
        }
    }

    /// The string the file stores for this mode.
    pub fn stored(self) -> &'static str {
        match self {
            DnsMode::System => "system",
            DnsMode::Doh => "doh",
            DnsMode::Dot => "dot",
            DnsMode::Nextdns => "nextdns",
        }
    }
}

impl DnsSettings {
    /// The stored values, checked and normalized, or why they cannot be used.
    ///
    /// What the settings window sends is checked here before it is stored: an
    /// endpoint that cannot be asked is refused rather than saved and failed
    /// at the first download.
    pub fn cleaned(&self) -> Result<DnsSettings, String> {
        let mode = DnsMode::from_stored(&self.mode);
        let mut clean = self.clone();
        clean.mode = mode.stored().to_string();
        match mode {
            DnsMode::System => {}
            DnsMode::Doh => clean.doh_url = check_doh_url(&self.doh_url)?,
            DnsMode::Dot => clean.dot_host = check_dot_host(&self.dot_host)?,
            DnsMode::Nextdns => {
                let profile = self.nextdns_profile.trim().to_ascii_lowercase();
                if !valid_nextdns_profile(&profile) {
                    return Err(
                        "A NextDNS configuration id is six hex digits, or empty for the default."
                            .to_string(),
                    );
                }
                clean.nextdns_profile = profile;
            }
        }
        Ok(clean)
    }

    /// The resolver for the stored values, or why they cannot be used.
    pub fn validated(&self) -> Result<Resolver, String> {
        let clean = self.cleaned()?;
        match DnsMode::from_stored(&clean.mode) {
            DnsMode::System => Ok(Resolver::system()),
            DnsMode::Doh => {
                let host = reqwest::Url::parse(&clean.doh_url)
                    .map(|parsed| parsed.host_str().unwrap_or("encrypted DNS").to_owned())
                    .unwrap_or_else(|_| "DNS over HTTPS".to_owned());
                Ok(Resolver::over_https(host, clean.doh_url))
            }
            DnsMode::Dot => {
                let host = check_dot_host(&clean.dot_host)?;
                Ok(Resolver::over_tls(host.clone(), host, 853))
            }
            DnsMode::Nextdns => Ok(Preset::by_id("nextdns")
                .expect("NextDNS is a shipped preset")
                .resolver(&clean.nextdns_profile)),
        }
    }

    /// The resolver the engine asks. A mode whose endpoint cannot be used
    /// reads as the system's: the caller is expected to have said why in the
    /// log, because silently changing the resolver is the one failure this
    /// file refuses to hide.
    pub fn resolver(&self) -> Resolver {
        self.validated().unwrap_or_else(|_| Resolver::system())
    }

    /// What the engine hands out as its choice.
    pub fn choice(&self) -> ResolverChoice {
        ResolverChoice {
            resolver: self.resolver(),
            fallback: self.fallback,
        }
    }
}

/// The endpoint of a DNS over HTTPS server, in the form it is stored:
/// `https://host[:port][/path]`, never with a user name or a password.
///
/// Returns the normalized URL. What is refused is what a resolver cannot
/// honour: a plain address, a name without a host, or credentials that would
/// otherwise be written down in plain local storage.
pub fn check_doh_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Enter the address of the DNS over HTTPS server.".to_string());
    }
    let mut parsed = reqwest::Url::parse(trimmed).map_err(|_| {
        "Use a full address, for example https://cloudflare-dns.com/dns-query.".to_string()
    })?;
    if parsed.scheme() != "https" {
        return Err("DNS over HTTPS needs an https:// address.".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Remove the user name and password from the address.".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("The address has no host.".to_string());
    }
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

/// The hostname of a DNS over TLS server, lowercased and without a trailing
/// dot.
///
/// A DNS name, not an address and not a URL: the certificate the server
/// presents is checked against this name, so it has to be the name the
/// server was given one. An address literal is refused — use DNS over HTTPS
/// for `https://1.1.1.1/dns-query`.
pub fn check_dot_host(host: &str) -> Result<String, String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("Enter the hostname of the DNS over TLS server.".to_string());
    }
    if host.parse::<IpAddr>().is_ok() {
        return Err(
            "DNS over TLS verifies a name, not an address. Use a hostname, or DNS over HTTPS."
                .to_string(),
        );
    }
    if host.len() > 253 {
        return Err("That hostname is too long.".to_string());
    }
    if !host.contains('.') {
        return Err("Use a full hostname, for example one.one.one.one.".to_string());
    }
    for label in host.split('.') {
        if label.len() > 63 {
            return Err(format!("The label \"{label}\" is too long."));
        }
        let bytes = label.as_bytes();
        let alnum = |b: u8| b.is_ascii_alphanumeric();
        let starts = bytes.first().map(|b| alnum(*b)).unwrap_or(false);
        let ends = bytes.last().map(|b| alnum(*b)).unwrap_or(false);
        if !starts || !ends {
            return Err(format!("The label \"{label}\" may not start or end with a hyphen."));
        }
        if !bytes.iter().all(|b| alnum(*b) || *b == b'-') {
            return Err(
                "The hostname may only carry letters, numbers, hyphens and dots.".to_string(),
            );
        }
    }
    Ok(host)
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

    #[test]
    fn a_doh_endpoint_needs_https_a_host_and_no_credentials() {
        assert_eq!(
            check_doh_url("https://cloudflare-dns.com/dns-query").unwrap(),
            "https://cloudflare-dns.com/dns-query"
        );
        assert_eq!(
            check_doh_url("https://1.1.1.1/dns-query?x=y#frag").unwrap(),
            "https://1.1.1.1/dns-query?x=y"
        );
        assert_eq!(
            check_doh_url("https://cloudflare-dns.com").unwrap(),
            "https://cloudflare-dns.com/"
        );
        assert!(check_doh_url("http://cloudflare-dns.com/dns-query").is_err());
        assert!(check_doh_url("https://").is_err());
        assert!(check_doh_url("cloudflare-dns.com/dns-query").is_err());
        assert!(check_doh_url("https://alice:secret@1.1.1.1/dns-query").is_err());
        assert!(check_doh_url("https://user@1.1.1.1/dns-query").is_err());
        assert!(check_doh_url("").is_err());
    }

    #[test]
    fn a_dot_hostname_is_a_name_not_an_address() {
        assert_eq!(check_dot_host(" One.One.One.One. ").unwrap(), "one.one.one.one");
        assert_eq!(check_dot_host("dns.google").unwrap(), "dns.google");
        assert_eq!(check_dot_host("ABC123.dns.nextdns.io").unwrap(), "abc123.dns.nextdns.io");
        assert!(check_dot_host("").is_err());
        assert!(check_dot_host("1.1.1.1").is_err());
        assert!(check_dot_host("::1").is_err());
        assert!(check_dot_host("one.one").is_ok());
        assert!(check_dot_host("one").is_err());
        assert!(check_dot_host("a..b").is_err());
        assert!(check_dot_host("-one.one.one.one").is_err());
        assert!(check_dot_host("one-.one.one.one").is_err());
        assert!(check_dot_host("under_score.one").is_err());
        assert!(check_dot_host(&format!("{}.example.com", "a".repeat(64))).is_err());
    }

    #[test]
    fn the_stored_values_build_the_resolver_the_window_chose() {
        let system = DnsSettings::default();
        assert_eq!(system.resolver(), Resolver::system());
        assert_eq!(system.mode, "system");

        let doh = DnsSettings {
            mode: "doh".into(),
            doh_url: "https://cloudflare-dns.com/dns-query".into(),
            ..DnsSettings::default()
        };
        assert_eq!(
            doh.validated().unwrap(),
            Resolver::over_https("cloudflare-dns.com", "https://cloudflare-dns.com/dns-query")
        );

        let dot = DnsSettings {
            mode: "dot".into(),
            dot_host: "DNS.Google".into(),
            ..DnsSettings::default()
        };
        assert_eq!(
            dot.validated().unwrap(),
            Resolver::over_tls("dns.google", "dns.google", 853)
        );

        let nextdns = DnsSettings {
            mode: "nextdns".into(),
            nextdns_profile: "a1b2c3".into(),
            ..DnsSettings::default()
        };
        assert_eq!(nextdns.validated().unwrap().address, "a1b2c3.dns.nextdns.io");
        let bad = DnsSettings {
            nextdns_profile: "zz".into(),
            ..nextdns
        };
        assert!(bad.validated().is_err());

        // A mode whose endpoint cannot be used reads as the system's, and the
        // choice carries the reader's fallback answer either way.
        let broken = DnsSettings {
            mode: "doh".into(),
            doh_url: "http://not-https.example.com/x".into(),
            fallback: true,
            ..DnsSettings::default()
        };
        assert_eq!(broken.resolver(), Resolver::system());
        assert!(broken.choice().fallback);
        assert!(!system.choice().fallback);
    }
}
