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
    Hosts, LookupFailure, Preset, Resolver, ResolverChoice, ResolverKind, Who, DnsMode,
    DnsSettings, PRESETS,
};
pub use pac::{PacError, ProxyRoute};
pub use resolver::{test as test_resolver, Dns};

use serde::{Deserialize, Serialize};

/// The builder every outbound HTTP client in the application starts from.
///
/// This is the one place a client is given the reader's network choices: the
/// proxy it goes through and the DNS resolver it looks names up with. The
/// resolver is the one in force (see [`resolver::set_current`]); `dns`
/// overrides it, for the tests that bring a resolver of their own. A client
/// that builds around this gets the reader's resolver by default, so this is
/// not optional plumbing — a call site that does not use it will silently
/// ignore the choice. A proxy the engine cannot dial is refused, never
/// silently ignored either: a silent drop is how a reader ends up downloading
/// directly while believing traffic goes through their proxy.
pub fn client_builder(
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
    Ok(builder)
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
        assert_eq!(Proxy::Script("function FindProxyForURL(){}".into()).name(), "Script");
        assert_eq!(
            Proxy::Script("function FindProxyForURL(){}".into()).script(),
            Some("function FindProxyForURL(){}")
        );
        assert_eq!(Proxy::Manual("p:8080".into()).script(), None);
    }
}
