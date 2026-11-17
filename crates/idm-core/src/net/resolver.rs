//! Asking the chosen DNS server a name, and what may happen when it cannot
//! or refuses to answer.
//!
//! This is the only file that opens a socket to a DNS server. Which server,
//! and when the system's may answer instead, is decided in [`super::dns`];
//! this is the asking itself, over DNS over TLS (RFC 7858), DNS over HTTPS
//! (RFC 8484) or plain TCP (RFC 7766 — never UDP, so nothing here ever looks
//! like a listening socket to a firewall).
//!
//! The names a public server cannot know — the hosts file, `localhost`, the
//! single-label names, the local suffixes — always go to the system's
//! resolver. A name the chosen resolver says does not exist never falls back
//! to the system's; only a resolver that could not be reached may, and only
//! when the reader allowed it.
//!
//! A DoT or DoH server named by hostname needs one bootstrap lookup through
//! the system, the same limitation every encrypted DNS has; it happens once
//! per resolver, not once per lookup.
//!
//! When a lookup fails, the log records the failure and the mode, never the
//! query: what the reader was downloading is not the log's business.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hickory_resolver::config::{
    ConnectionConfig, LookupIpStrategy, NameServerConfig, ResolveHosts, ResolverConfig,
    ResolverOpts, ServerOrderingStrategy,
};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::Resolver as HickoryResolver;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::dns::{
    is_local_name, may_fall_back, Hosts, LookupFailure, Resolver, ResolverChoice, ResolverKind,
};

/// How long one server gets to answer before the lookup is given up.
const DNS_TIMEOUT: Duration = Duration::from_secs(4);

/// The chosen resolver, as every HTTP client in the application asks through.
///
/// One of these is built when the settings are applied and shared by all of
/// them, so the resolver's cache is shared too.
#[derive(Debug, Clone)]
pub struct Dns {
    /// The names pinned to addresses by hand (the hosts file model),
    /// lowercased.
    pinned: HashMap<String, Vec<IpAddr>>,
    shared: Arc<Shared>,
}

struct Shared {
    choice: ResolverChoice,
    /// Certificates on top of the system's store (the tests' own CA). Empty in
    /// the application.
    extra_roots: Vec<Vec<u8>>,
    resolver: tokio::sync::OnceCell<Result<HickoryResolver<TokioRuntimeProvider>, String>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").finish()
    }
}

/// The resolver in force for the whole application.
///
/// The reader chose one resolver, so there is one: the application builds it
/// when the settings are applied and puts it here, and every client built
/// after that asks names through it. A client that builds its own without
/// this slot would quietly use the system's resolver, which is the one thing
/// the reader asked not to happen.
static CURRENT: std::sync::OnceLock<std::sync::Mutex<Option<Arc<Dns>>>> = std::sync::OnceLock::new();

fn slot() -> &'static std::sync::Mutex<Option<Arc<Dns>>> {
    CURRENT.get_or_init(|| std::sync::Mutex::new(None))
}

/// The resolver in force, if the reader chose one.
pub fn current() -> Option<Arc<Dns>> {
    slot().lock().expect("dns lock poisoned").clone()
}

/// Put a resolver in force. `None` means the system's own resolver.
pub fn set_current(dns: Option<Arc<Dns>>) {
    let mut guard = slot().lock().expect("dns lock poisoned");
    *guard = dns;
}

impl Dns {
    /// The resolver for `choice`, with no extra certificates.
    pub fn new(hosts: &Hosts, choice: &ResolverChoice) -> Self {
        Self::with_roots(hosts, choice, Vec::new())
    }

    /// The resolver for `choice`, additionally trusting `extra_roots` (a DER
    /// certificate each). The tests' own CA goes through here.
    pub fn with_roots(hosts: &Hosts, choice: &ResolverChoice, extra_roots: Vec<Vec<u8>>) -> Self {
        Self {
            pinned: hosts
                .entries()
                .map(|(name, ips)| (name.to_owned(), ips.to_vec()))
                .collect(),
            shared: Arc::new(Shared {
                choice: choice.clone(),
                extra_roots,
                resolver: tokio::sync::OnceCell::new(),
            }),
        }
    }

    /// The addresses of `host`. Local names and pinned names stay with the
    /// system's resolver and the pinned map; everything else asks the chosen
    /// resolver, falling back to the system's only as `choice` allows.
    ///
    /// The addresses are then held to the adapter the reader chose: an
    /// address the adapter cannot reach is dropped, and a chosen adapter that
    /// is gone stops the lookup instead of letting the download take the
    /// normal route.
    pub async fn addresses(&self, host: &str) -> Result<Vec<IpAddr>, String> {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if let Some(pinned) = self.pinned.get(&host) {
            return self.keep(pinned.clone());
        }
        let choice = &self.shared.choice;
        if choice.is_system() || is_local_name(&host) {
            let found = system(&host).await?;
            return self.keep(found);
        }
        match self.ask(&host).await {
            Ok(ips) => self.keep(ips),
            Err(failure) => {
                // The query stays out of the log; the failure and the mode do not.
                crate::log::write(
                    "warn",
                    &format!(
                        "a lookup through {} failed: {}",
                        choice.resolver.label(),
                        failure.as_str()
                    ),
                );
                if failure == Failure::Unreachable
                    && may_fall_back(choice, LookupFailure::Unreachable)
                {
                    let found = system(&host).await?;
                    return self.keep(found);
                }
                Err(failure.as_str().to_string())
            }
        }
    }

    /// The addresses the chosen adapter can reach, or the reason it cannot
    /// carry them.
    fn keep(&self, ips: Vec<IpAddr>) -> Result<Vec<IpAddr>, String> {
        let route = crate::net::adapter::route();
        match route {
            crate::net::adapter::Route::Any => Ok(ips),
            crate::net::adapter::Route::Down { label } => Err(format!(
                "the network adapter {label} is not connected, so nothing is downloaded"
            )),
            crate::net::adapter::Route::Bound(_) => {
                let kept = route.keep(ips);
                if kept.is_empty() {
                    return Err("the chosen network adapter cannot reach the address of the server"
                        .to_string());
                }
                Ok(kept)
            }
        }
    }

    async fn ask(&self, host: &str) -> Result<Vec<IpAddr>, Failure> {
        let built = self
            .shared
            .resolver
            .get_or_init(|| build(&self.shared.choice, &self.shared.extra_roots))
            .await;
        let resolver = built.as_ref().map_err(|_| Failure::Unreachable)?;
        match resolver.lookup_ip(host).await {
            Ok(found) => {
                let ips: Vec<IpAddr> = found.iter().collect();
                if ips.is_empty() {
                    Err(Failure::NoSuchName)
                } else {
                    Ok(ips)
                }
            }
            Err(error) if error.is_nx_domain() || error.is_no_records_found() => {
                Err(Failure::NoSuchName)
            }
            Err(_) => Err(Failure::Unreachable),
        }
    }
}

impl Resolve for Dns {
    fn resolve(&self, name: Name) -> Resolving {
        let dns = self.clone();
        let host = name.as_str().to_ascii_lowercase();
        Box::pin(async move {
            let ips = dns.addresses(&host).await.map_err(|error| {
                Box::new(std::io::Error::other(error)) as Box<dyn std::error::Error + Send + Sync>
            })?;
            // Port 0: reqwest substitutes the URL's port at connect time.
            let addrs: Addrs = Box::new(ips.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

/// How long `choice` takes to answer for `host`, and with what.
///
/// Never falls back to the system's resolver: the settings window uses this
/// to show the reader what they actually chose, and a hidden fallback is how
/// a dead resolver goes unnoticed.
pub async fn test(
    choice: &ResolverChoice,
    host: &str,
    extra_roots: &[Vec<u8>],
) -> Result<(Duration, Vec<IpAddr>), String> {
    let strict = ResolverChoice {
        resolver: choice.resolver.clone(),
        fallback: false,
    };
    let started = Instant::now();
    let ips = if strict.is_system() {
        system(host).await?
    } else {
        let resolver = build(&strict, extra_roots)
            .await
            .map_err(|error| format!("the resolver could not be set up: {error}"))?;
        match resolver.lookup_ip(host).await {
            Ok(found) => {
                let ips: Vec<IpAddr> = found.iter().collect();
                if ips.is_empty() {
                    return Err("the resolver says the name does not exist".to_string());
                }
                ips
            }
            Err(error) if error.is_nx_domain() || error.is_no_records_found() => {
                return Err("the resolver says the name does not exist".to_string());
            }
            Err(_) => return Err("the resolver could not be reached".to_string()),
        }
    };
    Ok((started.elapsed(), ips))
}

/// Why a lookup through the chosen resolver failed. The strings stay free of
/// the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// The resolver answered: there is no such name.
    NoSuchName,
    /// The resolver could not be asked, or did not answer.
    Unreachable,
}

impl Failure {
    fn as_str(self) -> &'static str {
        match self {
            Failure::NoSuchName => "the resolver says the name does not exist",
            Failure::Unreachable => "the resolver could not be reached",
        }
    }
}

async fn system(host: &str) -> Result<Vec<IpAddr>, String> {
    Ok(tokio::net::lookup_host((host, 0))
        .await
        .map_err(|error| format!("the system's resolver could not be asked: {error}"))?
        .map(|address| address.ip())
        .collect())
}

/// The chosen resolver as a hickory resolver.
async fn build(
    choice: &ResolverChoice,
    extra_roots: &[Vec<u8>],
) -> Result<HickoryResolver<TokioRuntimeProvider>, String> {
    let label = choice.resolver.label();
    let setup = |why: String| format!("{label}: {why}");
    let port = choice.resolver.port;
    let servers: Vec<NameServerConfig> = match choice.resolver.kind {
        ResolverKind::System => {
            return Err(setup("the system's resolver is not asked through here".into()))
        }
        ResolverKind::Plain => {
            let ips = server_ips(&choice.resolver.address, label.as_str()).await?;
            self::with_port(ips, port, |ip, port| {
                NameServerConfig::new(
                    ip,
                    true,
                    vec![connection(ConnectionConfig::tcp(), port)],
                )
            })
        }
        ResolverKind::OverTls => {
            let name = choice.resolver.address.trim().to_owned();
            let ips = server_ips(&name, label.as_str()).await?;
            self::with_port(ips, port, |ip, port| {
                NameServerConfig::new(
                    ip,
                    true,
                    vec![connection(
                        ConnectionConfig::tls(Arc::from(name.as_str())),
                        port,
                    )],
                )
            })
        }
        ResolverKind::OverHttps => https_servers(&choice.resolver, label.as_str()).await?,
    };
    if servers.is_empty() {
        return Err(setup("no server the resolver can reach".into()));
    }
    let tls = tls_config(extra_roots).map_err(setup)?;
    let mut builder = HickoryResolver::builder_with_config(
        ResolverConfig::from_name_servers(servers),
        TokioRuntimeProvider::default(),
    )
    .with_tls_config(tls);
    let options: &mut ResolverOpts = builder.options_mut();
    options.timeout = DNS_TIMEOUT;
    options.attempts = 1;
    options.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    // Asked in the order configured, not reordered by past answers.
    options.server_ordering_strategy = ServerOrderingStrategy::UserProvidedOrder;
    // The hosts file is the reader's own word, whichever resolver answers.
    options.use_hosts_file = ResolveHosts::Always;
    builder.build().map_err(|error| setup(error.to_string()))
}

fn connection(mut config: ConnectionConfig, port: u16) -> ConnectionConfig {
    config.port = port;
    config
}

/// Servers with the IPv4 ones first, so a machine without IPv6 does not spend
/// a timeout on an IPv6 server before the IPv4 ones answer.
fn ipv4_first(ips: Vec<IpAddr>) -> Vec<IpAddr> {
    let (v4, v6): (Vec<IpAddr>, Vec<IpAddr>) = ips.into_iter().partition(|ip| ip.is_ipv4());
    v4.into_iter().chain(v6).collect()
}

fn with_port(
    ips: Vec<IpAddr>,
    port: u16,
    make: impl Fn(IpAddr, u16) -> NameServerConfig,
) -> Vec<NameServerConfig> {
    ipv4_first(ips).into_iter().map(|ip| make(ip, port)).collect()
}

async fn https_servers(resolver: &Resolver, label: &str) -> Result<Vec<NameServerConfig>, String> {
    let url = reqwest::Url::parse(&resolver.address)
        .map_err(|error| format!("{label}: {error}"))?;
    if url.scheme() != "https" {
        return Err(format!("{label}: not an https:// address"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("{label}: no server in the address"))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = url.port().unwrap_or(443);
    let ips: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => server_ips(&host, label).await?,
    };
    let path: Arc<str> = Arc::from(if url.path().is_empty() {
        "/"
    } else {
        url.path()
    });
    Ok(with_port(ips, port, |ip, port| {
        NameServerConfig::new(
            ip,
            true,
            vec![connection(
                ConnectionConfig::https(Arc::from(host.as_str()), Some(Arc::clone(&path))),
                port,
            )],
        )
    }))
}

/// The addresses of a server given as a literal address or a hostname. A
/// hostname needs one bootstrap lookup through the system's resolver: an
/// encrypted DNS cannot find its own server.
async fn server_ips(address: &str, label: &str) -> Result<Vec<IpAddr>, String> {
    let address = address.trim();
    if let Ok(ip) = address.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let host = address.trim_start_matches('[').trim_end_matches(']');
    Ok(tokio::net::lookup_host((host, 0))
        .await
        .map_err(|error| format!("{label}: the bootstrap lookup failed: {error}"))?
        .map(|entry| entry.ip())
        .collect())
}

/// The client configuration for DoT and DoH: the operating system's verifier,
/// as reqwest uses for HTTPS.
///
/// The certificates in `extra` replace the system's store rather than adding
/// to it: they are an authority of the caller's own (the tests'), and a
/// system store would only add noise to what is being proven.
fn tls_config(extra: &[Vec<u8>]) -> Result<rustls::ClientConfig, String> {
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    if !extra.is_empty() {
        let mut roots = rustls::RootCertStore::empty();
        for certificate in extra {
            roots
                .add(rustls_pki_types::CertificateDer::from(certificate.clone()))
                .map_err(|error| format!("the certificate could not be trusted: {error}"))?;
        }
        return Ok(rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| error.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth());
    }
    use rustls_platform_verifier::Verifier;
    let verifier = Verifier::new(provider.clone())
        .map_err(|error| format!("the system's certificate store could not be read: {error}"))?;
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

#[cfg(test)]
mod tests;
