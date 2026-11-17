//! The network adapter the connections leave through, and the state machine
//! that fails closed when it cannot carry them.
//!
//! A socket bound to an adapter before it connects cannot leave through
//! another one: the operating system refuses to send its packets elsewhere.
//! Binding therefore *is* the kill switch, and it fails closed — when the
//! adapter is gone, has lost its address, or carries no address of the
//! family the server has, the connection fails instead of quietly taking the
//! normal route.
//!
//! Each system needs its own call, and the calls are in [`socket`]:
//!
//! - **Windows**: `setsockopt(IP_UNICAST_IF, index)`; the adapter's
//!   addresses are read with `GetAdaptersAddresses` (iphlpapi.h). From Vista
//!   on, Windows uses the strong host model, so a packet with an adapter's
//!   source address only leaves through that adapter.
//! - **Linux**: `setsockopt(SO_BINDTODEVICE, name)` (Linux 5.7 and later
//!   allow it without privileges). An address alone would not hold: Linux
//!   routes by destination, so a packet with the VPN's address could still
//!   leave through the Ethernet adapter.
//! - **macOS**: `setsockopt(IP_BOUND_IF, index)`, and `IPV6_BOUND_IF` for
//!   IPv6.
//!
//! What is modelled here is the decision each of those calls would make:
//! which address is bound, which family is dropped, and when a connection is
//! refused rather than sent around the adapter.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[cfg(windows)]
mod windows;

mod socket;

pub use socket::open;

/// The platform call that would hold a socket to the adapter.
#[cfg(windows)]
pub const BINDING_CALL: &str =
    "setsockopt(IP_UNICAST_IF, index); the addresses come from GetAdaptersAddresses (iphlpapi.h)";

/// The platform call that would hold a socket to the adapter.
#[cfg(target_os = "linux")]
pub const BINDING_CALL: &str = "setsockopt(SO_BINDTODEVICE, name)";

/// The platform call that would hold a socket to the adapter.
#[cfg(target_os = "macos")]
pub const BINDING_CALL: &str = "setsockopt(IP_BOUND_IF, index) (IPV6_BOUND_IF for IPv6)";

/// The platform call that would hold a socket to the adapter.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub const BINDING_CALL: &str = "this system has no way to hold a socket to an adapter";

/// The state of the chosen adapter, for one address family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterState {
    /// The adapter is there and carries that family: the connection may go
    /// through it.
    Ready,
    /// The adapter is not there, or has lost every address: nothing may
    /// connect.
    Down,
    /// The adapter is there but carries the other family: it cannot reach
    /// this one.
    WrongFamily,
}

/// What a connection to one address may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No adapter is chosen: the system decides, the normal route.
    Any,
    /// The connection is to this computer: it never leaves it, so it is not
    /// bound.
    Local,
    /// The connection is bound to the adapter: it can only leave through it.
    Bind {
        /// The address the socket would bind to.
        local: IpAddr,
    },
    /// The connection fails: the adapter is not there.
    Down {
        /// The name of the adapter.
        label: String,
    },
    /// The connection fails: the adapter carries no address of this family.
    WrongFamily {
        /// The name of the adapter.
        label: String,
        /// The address that could not be reached.
        ip: IpAddr,
    },
}

/// The chosen adapter as it is now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Binding {
    /// Its name as the system shows it ("Ethernet", "ProtonVPN", "wg0"), for
    /// the messages.
    pub label: String,
    /// The system's name for it: what Linux binds (`wg0`).
    pub name: String,
    /// Its interface index: what macOS binds.
    pub index: u32,
    /// Its IPv4 address, when it has one.
    pub v4: Option<Ipv4Addr>,
    /// Its IPv6 address, when it has one.
    pub v6: Option<Ipv6Addr>,
}

impl Binding {
    /// The state of the adapter for a connection to `ip`.
    pub fn state_for(&self, ip: IpAddr) -> AdapterState {
        if ip.is_loopback() {
            return AdapterState::Ready;
        }
        if self.v4.is_none() && self.v6.is_none() {
            return AdapterState::Down;
        }
        match self.local_for(ip) {
            Some(_) => AdapterState::Ready,
            None => AdapterState::WrongFamily,
        }
    }

    /// The address the socket would bind to for a connection to `ip`.
    pub fn local_for(&self, ip: IpAddr) -> Option<IpAddr> {
        match ip {
            IpAddr::V4(_) => self.v4.map(IpAddr::V4),
            IpAddr::V6(_) => self.v6.map(IpAddr::V6),
        }
    }

    /// The decision for a connection to `ip`.
    ///
    /// The answer is "no" rather than "some other way" whenever the adapter
    /// cannot carry the connection: that is what makes the choice a kill
    /// switch rather than a preference.
    pub fn decide(&self, ip: IpAddr) -> Decision {
        if ip.is_loopback() {
            return Decision::Local;
        }
        match self.state_for(ip) {
            AdapterState::Down => Decision::Down {
                label: self.label.clone(),
            },
            AdapterState::WrongFamily => Decision::WrongFamily {
                label: self.label.clone(),
                ip,
            },
            AdapterState::Ready => match self.local_for(ip) {
                Some(local) => Decision::Bind { local },
                None => Decision::WrongFamily {
                    label: self.label.clone(),
                    ip,
                },
            },
        }
    }

    /// `ips` without the addresses of the family the adapter cannot reach.
    pub fn keep(&self, ips: Vec<IpAddr>) -> Vec<IpAddr> {
        ips.into_iter()
            .filter(|ip| self.state_for(*ip) == AdapterState::Ready)
            .collect()
    }
}

/// How the connections leave this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Route {
    /// No adapter is chosen: the system decides (the default).
    #[default]
    Any,
    /// Through this adapter only.
    Bound(Binding),
    /// An adapter is chosen but it is not there or has no address: nothing
    /// may connect.
    Down {
        /// The name of the adapter.
        label: String,
    },
}

impl Route {
    /// The decision for a connection to `ip`.
    pub fn decide(&self, ip: IpAddr) -> Decision {
        match self {
            Route::Any => Decision::Any,
            Route::Down { label } => Decision::Down {
                label: label.clone(),
            },
            Route::Bound(b) => b.decide(ip),
        }
    }

    /// `ips` without the addresses this route cannot reach.
    pub fn keep(&self, ips: Vec<IpAddr>) -> Vec<IpAddr> {
        match self {
            Route::Any => ips,
            Route::Down { .. } => Vec::new(),
            Route::Bound(b) => b.keep(ips),
        }
    }

    /// The name of the adapter, when one is chosen.
    pub fn label(&self) -> Option<&str> {
        match self {
            Route::Any => None,
            Route::Bound(b) => Some(&b.label),
            Route::Down { label } => Some(label),
        }
    }
}

/// A network adapter of this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// The system's identity for it, and what the settings file keeps.
    ///
    /// Windows' GUID survives a rename and a restart; the display name
    /// changes and the interface index does not have to be the same after a
    /// reboot, so neither of those is an identity.
    pub id: String,
    /// The name shown in Network Connections ("Ethernet", "ProtonVPN"); on
    /// Linux and macOS the interface's name ("wg0", "utun4").
    pub name: String,
    /// The system's index for it.
    pub index: u32,
    /// Whether the system says it is up. A VPN that is off is listed, so the
    /// reader can see it and see that it is carrying nothing.
    pub up: bool,
    /// Its addresses that can reach other computers (no link-local
    /// 169.254.x.x or fe80::).
    pub v4: Vec<Ipv4Addr>,
    /// Its IPv6 addresses that can reach other computers.
    pub v6: Vec<Ipv6Addr>,
    /// Its link-local addresses: only a computer on the same cable answers
    /// them.
    pub local_only: Vec<IpAddr>,
}

impl Adapter {
    /// The binding for this adapter, or `None` when it has no address at all
    /// and therefore nothing may connect through it.
    pub fn binding(&self) -> Option<Binding> {
        if self.v4.is_empty() && self.v6.is_empty() {
            return None;
        }
        Some(Binding {
            label: self.name.clone(),
            name: self.name.clone(),
            index: self.index,
            v4: self.v4.first().copied(),
            // A global address first: a unique-local one (fd00::/8) is used
            // inside VPNs too, and it is not the address the world sees.
            v6: self
                .v6
                .iter()
                .find(|v| !v.is_unique_local())
                .or(self.v6.first())
                .copied(),
        })
    }
}

/// The route for the adapter named by `choice`.
///
/// `choice` is "" (any), the adapter's identity (its GUID on Windows), its
/// name (any case), or one of its addresses (that address is bound, even a
/// link-local one). A choice that is not there, or has no address, is
/// [`Route::Down`]: it never becomes [`Route::Any`], because a reader who
/// chose an adapter wants that adapter and not a silent fallback to the
/// normal route.
pub fn route_for(choice: &str, adapters: &[Adapter]) -> Route {
    let choice = choice.trim();
    if choice.is_empty() {
        return Route::Any;
    }
    let down = || Route::Down {
        label: choice.to_owned(),
    };
    if let Ok(ip) = choice.parse::<IpAddr>() {
        let found = adapters.iter().find(|a| {
            a.v4.iter().any(|v| IpAddr::V4(*v) == ip)
                || a.v6.iter().any(|v| IpAddr::V6(*v) == ip)
                || a.local_only.contains(&ip)
        });
        let Some(a) = found else {
            return down();
        };
        let (v4, v6) = match ip {
            IpAddr::V4(v) => (Some(v), None),
            IpAddr::V6(v) => (None, Some(v)),
        };
        return Route::Bound(Binding {
            label: format!("{} ({ip})", a.name),
            name: a.name.clone(),
            index: a.index,
            v4,
            v6,
        });
    }
    // The identity first: it is what the settings file keeps, and a renamed
    // adapter is still the adapter the reader chose.
    let found = adapters
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(choice))
        .or_else(|| {
            adapters
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(choice))
        });
    let Some(a) = found else {
        return down();
    };
    match a.binding() {
        Some(b) => Route::Bound(b),
        None => Route::Down {
            label: a.name.clone(),
        },
    }
}

/// The computer's network adapters, in the system's order.
///
/// Adapters without an address are listed too: a VPN that is off is a choice
/// the reader made, and it must be visible as "not connected" rather than
/// missing.
pub fn adapters() -> Vec<Adapter> {
    #[cfg(windows)]
    {
        windows::adapters()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The route in force for the whole application.
///
/// The reader chose one adapter, so there is one: the application builds the
/// route when the settings are applied and puts it here, and every client
/// built after that leaves through it. A route that is [`Route::Down`] is the
/// kill switch: nothing may connect.
static CURRENT: std::sync::OnceLock<std::sync::Mutex<Route>> = std::sync::OnceLock::new();

fn slot() -> &'static std::sync::Mutex<Route> {
    CURRENT.get_or_init(|| std::sync::Mutex::new(Route::Any))
}

/// The route in force.
pub fn route() -> Route {
    slot().lock().expect("route lock poisoned").clone()
}

/// Put a route in force. [`Route::Down`] is what a chosen adapter that is
/// gone becomes.
pub fn set_route(route: Route) {
    let mut guard = slot().lock().expect("route lock poisoned");
    *guard = route;
}

/// The bytes of the interface option for a connection to `ip`.
///
/// Windows takes the index in network byte order for IPv4 and in host byte
/// order for IPv6, and the two options have the same number.
pub fn interface_option(ip: IpAddr, index: u32) -> Option<(i32, [u8; 4])> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Networking::WinSock::{IPV6_UNICAST_IF, IP_UNICAST_IF};
        match ip {
            IpAddr::V4(_) => Some((IP_UNICAST_IF, index.to_be_bytes())),
            IpAddr::V6(_) => Some((IPV6_UNICAST_IF, index.to_ne_bytes())),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (ip, index);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nic(name: &str, v4: &[&str], v6: &[&str], local: &[&str]) -> Adapter {
        Adapter {
            id: format!("{{{name}}}"),
            name: name.into(),
            index: 7,
            up: true,
            v4: v4.iter().map(|s| s.parse().unwrap()).collect(),
            v6: v6.iter().map(|s| s.parse().unwrap()).collect(),
            local_only: local.iter().map(|s| s.parse().unwrap()).collect(),
        }
    }

    #[test]
    fn the_identity_finds_the_adapter_the_reader_chose() {
        let list = [nic("Ethernet", &["192.168.1.11"], &[], &["fe80::1"])];
        // The identity is what the settings file keeps, and it survives a
        // rename: the adapter is still the one the reader chose.
        let Route::Bound(b) = route_for("{Ethernet}", &list) else {
            panic!("the adapter is there");
        };
        assert_eq!(b.label, "Ethernet");
        // The name still works, for a file written by hand.
        let Route::Bound(b) = route_for("ethernet", &list) else {
            panic!("the adapter is there");
        };
        assert_eq!(b.v4, Some("192.168.1.11".parse().unwrap()));
    }

    #[test]
    fn the_route_follows_the_adapter_through_bound_lost_and_recovered() {
        let up = [nic("VPN", &["10.8.0.2"], &[], &[])];
        let gone: [Adapter; 0] = [];
        // Bound: the adapter is there and carries the address.
        assert!(matches!(route_for("{VPN}", &up), Route::Bound(_)));
        // Lost: the adapter is gone. The route is down, never the normal one.
        assert!(matches!(route_for("{VPN}", &gone), Route::Down { .. }));
        // Recovered: it is back, and the route is bound again.
        assert!(matches!(route_for("{VPN}", &up), Route::Bound(_)));
    }

    #[cfg(windows)]
    #[test]
    fn the_interface_option_takes_the_index_in_its_own_byte_order() {
        // Windows takes the IPv4 index in network byte order and the IPv6 one
        // in host byte order: the same number, two orders.
        let (option4, bytes4) =
            interface_option("93.184.216.34".parse().unwrap(), 7).expect("the IPv4 option");
        let (option6, bytes6) =
            interface_option("2606:2800::1".parse().unwrap(), 7).expect("the IPv6 option");
        #[cfg(windows)]
        {
            use windows_sys::Win32::Networking::WinSock::{IPV6_UNICAST_IF, IP_UNICAST_IF};
            assert_eq!(option4, IP_UNICAST_IF);
            assert_eq!(option6, IPV6_UNICAST_IF);
            assert_eq!(bytes4, [0, 0, 0, 7]);
            assert_eq!(bytes6, 7u32.to_ne_bytes());
        }
        #[cfg(not(windows))]
        {
            let _ = (option4, bytes4, option6, bytes6);
        }
    }

    #[test]
    fn a_missing_or_addressless_adapter_is_down_never_any() {
        let list = [
            nic("Ethernet", &["192.168.1.11"], &[], &["fe80::1"]),
            nic("ProtonVPN", &[], &[], &[]),
        ];
        assert_eq!(route_for("", &list), Route::Any);
        assert!(matches!(route_for("WireGuard", &list), Route::Down { .. }));
        assert!(matches!(route_for("ProtonVPN", &list), Route::Down { .. }));
        assert!(matches!(route_for("10.9.9.9", &list), Route::Down { .. }));
        let Route::Bound(b) = route_for("ethernet", &list) else {
            panic!("Ethernet is up");
        };
        assert_eq!(b.v4, Some("192.168.1.11".parse().unwrap()));
        assert_eq!(b.v6, None);
        // An address picks its adapter, even a link-local one.
        let Route::Bound(b) = route_for("fe80::1", &list) else {
            panic!("fe80::1 is Ethernet's");
        };
        assert_eq!((b.v4, b.v6), (None, Some("fe80::1".parse().unwrap())));
    }

    #[test]
    fn the_adapter_binds_what_it_carries_and_refuses_the_rest() {
        let b = Binding {
            label: "VPN".into(),
            v4: Some("10.8.0.2".parse().unwrap()),
            ..Default::default()
        };
        let ip4: IpAddr = "93.184.216.34".parse().unwrap();
        let ip6: IpAddr = "2606:2800::1".parse().unwrap();

        assert_eq!(b.state_for(ip4), AdapterState::Ready);
        assert_eq!(b.state_for(ip6), AdapterState::WrongFamily);
        assert_eq!(
            b.decide(ip4),
            Decision::Bind {
                local: "10.8.0.2".parse().unwrap()
            }
        );
        assert!(matches!(b.decide(ip6), Decision::WrongFamily { .. }));
        // A connection to this computer never leaves it, so it is not bound.
        assert_eq!(b.decide("127.0.0.1".parse().unwrap()), Decision::Local);
        assert_eq!(b.decide("::1".parse().unwrap()), Decision::Local);
        // The addresses of the other family are dropped, not left to chance.
        assert_eq!(b.keep(vec![ip6, ip4]), vec![ip4]);
    }

    #[test]
    fn an_adapter_without_an_address_fails_closed() {
        let down = Route::Down {
            label: "VPN".into(),
        };
        assert_eq!(
            down.decide("127.0.0.1".parse().unwrap()),
            Decision::Down {
                label: "VPN".into()
            }
        );
        assert!(down.keep(vec!["127.0.0.1".parse().unwrap()]).is_empty());
        // An adapter that is there but has no address at all is the same.
        let empty = Binding {
            label: "VPN".into(),
            ..Default::default()
        };
        assert_eq!(
            empty.state_for("1.2.3.4".parse().unwrap()),
            AdapterState::Down
        );
        assert!(matches!(
            empty.decide("1.2.3.4".parse().unwrap()),
            Decision::Down { .. }
        ));
    }

    #[test]
    fn without_an_adapter_the_system_decides() {
        let any = Route::Any;
        assert_eq!(any.decide("1.2.3.4".parse().unwrap()), Decision::Any);
        assert_eq!(any.label(), None);
    }
}
