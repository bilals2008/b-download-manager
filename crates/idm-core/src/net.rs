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
//! they drive, and the rest of the engine acts on those.

pub mod adapter;
pub mod dns;
pub mod pac;

pub use adapter::{Adapter, AdapterState, Binding, Decision, Route};
pub use dns::{
    Hosts, LookupFailure, Preset, Resolver, ResolverChoice, ResolverKind, Who, PRESETS,
};
pub use pac::{PacError, ProxyRoute};

use serde::{Deserialize, Serialize};

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
