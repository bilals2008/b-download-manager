//! The network the engine reaches the servers through.
//!
//! The proxy, the name resolution and the adapter the connections leave
//! through are one choice: they are told apart here so the settings window
//! can offer them apart.
//!
//! - [`dns`] — the resolver, its address and port, and the rules about when
//!   it may be asked: the hosts file and the local names always stay with
//!   the system's resolver.
//! - [`pac`] — the proxy auto-config script, and the small JavaScript
//!   evaluator that answers `FindProxyForURL(url, host)` without a
//!   JavaScript engine.
//! - [`adapter`] — the network adapter the connections are bound to, and the
//!   state machine that fails closed when it cannot carry them.
//!
//! Nothing here opens a socket: these are the choices and the decisions
//! they drive, and the rest of the engine acts on those — apart from
//! [`resolver`], which is the one place a DNS socket is opened.

pub mod adapter;
pub mod dns;
pub mod pac;
pub mod resolver;

pub use adapter::{Adapter, AdapterState, Binding, Decision, Route};
pub use dns::{
    DnsMode, DnsSettings, Hosts, LookupFailure, Preset, Resolver, ResolverChoice, ResolverKind,
    Who, PRESETS,
};
pub use pac::{PacError, ProxyRoute};
pub use resolver::{test as test_resolver, Dns};

use serde::{Deserialize, Serialize};

/// Whether a host is this computer: a connection to it never leaves, so it
/// is not held to an adapter.
pub fn is_this_computer(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The clients a download may use.
///
/// A proxy auto-config script decides per address, and a client carries one
/// proxy, so the engine keeps a client for each proxy the script names and
/// hands out the one the script chose. Without a script the reader's own
/// proxy setting is the route, as it always was.
#[derive(Debug)]
pub struct Clients {
    /// The client for the route the reader set.
    base: reqwest::Client,
    /// The client that goes straight to the server: what a script's
    /// `DIRECT` means.
    direct: reqwest::Client,
    /// One client per proxy the script named.
    by_proxy: std::sync::Mutex<std::collections::HashMap<String, reqwest::Client>>,
}

impl Clone for Clients {
    fn clone(&self) -> Self {
        // The clients are cheap to clone and the map is only a cache: the
        // clone starts with the routes it knows and learns the rest again.
        Self {
            base: self.base.clone(),
            direct: self.direct.clone(),
            by_proxy: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Clients {
    /// The clients for a download: the one the reader's setting gives, and
    /// the one that goes straight to the server.
    pub fn new(base: reqwest::Client) -> Result<Self, String> {
        let direct = client_builder(None, None, None)?
            .no_proxy()
            .build()
            .map_err(|error| {
                format!("the client that goes straight to the server could not be made: {error}")
            })?;
        Ok(Self {
            base,
            direct,
            by_proxy: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// The client for this address: the script decides when one is in
    /// force.
    ///
    /// A script that cannot answer is an error the caller sees: a download
    /// must not quietly take a route the script did not choose.
    pub fn for_url(&self, url: &str) -> Result<reqwest::Client, String> {
        if pac::script().is_none() {
            return Ok(self.base.clone());
        }
        match pac::route_for(url).map_err(|error| error.to_string())? {
            pac::ProxyRoute::Direct => Ok(self.direct.clone()),
            pac::ProxyRoute::Proxy(address) => self.client_for(&address, url),
        }
    }

    /// The client that goes through the proxy the script named.
    fn client_for(&self, address: &str, url: &str) -> Result<reqwest::Client, String> {
        let mut known = self.by_proxy.lock().expect("proxy lock poisoned");
        if let Some(client) = known.get(address) {
            return Ok(client.clone());
        }
        // A credential in the script's answer is refused, as the static
        // proxy's is: it would be written down in the request's own headers.
        if let Ok(parsed) = reqwest::Url::parse(address) {
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(
                    "the proxy script chose a proxy with a user name or a password".to_string(),
                );
            }
        }
        let host = reqwest::Url::parse(url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned));
        let client = client_builder(host.as_deref(), Some(address), None)?
            .build()
            .map_err(|error| {
                format!(
                    "the proxy script chose an address that cannot be used: {address} ({error})"
                )
            })?;
        known.insert(address.to_owned(), client.clone());
        Ok(client)
    }
}

/// The builder every outbound HTTP client in the application starts from.
///
/// This is the one place a client is given the reader's network choices: the
/// proxy it goes through, the DNS resolver it looks names up with, and the
/// adapter it leaves through. The resolver is the one in force (see
/// [`resolver::set_current`]); `dns` overrides it, for the tests that bring a
/// resolver of their own. A client that builds around this gets the reader's
/// choices by default, so this is not optional plumbing — a call site that
/// does not use it will silently ignore them. A proxy the engine cannot dial
/// is refused, never silently ignored either: a silent drop is how a reader
/// ends up downloading directly while believing traffic goes through their
/// proxy.
///
/// `target` is the host the client will ask, when the caller knows it: a
/// connection to this computer, or through a proxy on this computer, is not
/// held to the adapter — it never leaves, and binding it would break it.
///
/// A chosen adapter that is gone refuses the client: that is the kill
/// switch, and it fails closed.
pub fn client_builder(
    target: Option<&str>,
    proxy: Option<&str>,
    dns: Option<&std::sync::Arc<resolver::Dns>>,
) -> Result<reqwest::ClientBuilder, String> {
    let mut builder = reqwest::Client::builder();
    if let Some(proxy) = proxy.map(str::trim).filter(|proxy| !proxy.is_empty()) {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|error| format!("the proxy address is not usable: {error}"))?;
        builder = builder.proxy(proxy);
    }
    let dns = dns.cloned().or_else(resolver::current);
    if let Some(dns) = dns {
        builder = builder.dns_resolver(dns);
    }
    builder = bind_to_adapter(builder, target, proxy)?;
    Ok(builder)
}

/// Hold a client to the adapter the reader chose.
///
/// Windows binds the adapter's own address: from Vista on, a packet with that
/// source address leaves only through that adapter. The systems that route by
/// destination bind the interface itself.
fn bind_to_adapter(
    builder: reqwest::ClientBuilder,
    target: Option<&str>,
    proxy: Option<&str>,
) -> Result<reqwest::ClientBuilder, String> {
    let route = adapter::route();
    if matches!(route, adapter::Route::Any) {
        return Ok(builder);
    }
    // A connection that does not leave this computer is not held to the
    // adapter: it cannot, and binding it would only break it.
    let local = target.is_some_and(is_this_computer)
        || proxy
            .and_then(|proxy| reqwest::Url::parse(proxy).ok())
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| is_this_computer(&host));
    if local {
        return Ok(builder);
    }
    match route {
        adapter::Route::Down { label } => Err(format!(
            "the network adapter {label} is not connected, so nothing is downloaded"
        )),
        adapter::Route::Bound(binding) => {
            #[cfg(windows)]
            {
                let Some(local) = binding
                    .v4
                    .map(std::net::IpAddr::V4)
                    .or(binding.v6.map(std::net::IpAddr::V6))
                else {
                    return Err(format!(
                        "the network adapter {} has no address to leave from",
                        binding.label
                    ));
                };
                Ok(builder.local_address(local))
            }
            #[cfg(not(windows))]
            {
                Ok(builder)
            }
        }
        adapter::Route::Any => Ok(builder),
    }
}

/// The proxy the connections go through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Proxy {
    /// No proxy at all.
    None,
    /// The proxy of the operating system.
    System,
    /// A proxy the reader named.
    Manual(String),
    /// A proxy script that decides per address.
    Script(String),
}

impl Proxy {
    /// The name of the kind, as the settings window shows it.
    pub fn name(&self) -> &'static str {
        match self {
            Proxy::None => "None",
            Proxy::System => "System",
            Proxy::Manual(_) => "Manual",
            Proxy::Script(_) => "Script",
        }
    }

    /// The script, when this is a script proxy.
    pub fn script(&self) -> Option<&str> {
        match self {
            Proxy::Script(script) => Some(script),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_of_proxy_are_told_apart() {
        assert_eq!(Proxy::None.name(), "None");
        assert_eq!(
            Proxy::Script("function FindProxyForURL(){}".into()).name(),
            "Script"
        );
        assert_eq!(
            Proxy::Script("function FindProxyForURL(){}".into()).script(),
            Some("function FindProxyForURL(){}")
        );
        assert_eq!(Proxy::Manual("p:8080".into()).script(), None);
    }
}
