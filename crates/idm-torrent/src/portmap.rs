//! The router's port mapping: UPnP first, NAT-PMP when no IGD answers.
//!
//! One task holds the mapping for the client: it asks, refreshes before the
//! lease runs out, and gives the mapping back when the reader turns the
//! choice off or the application closes. A network with no gateway that
//! speaks either protocol is a normal result, said plainly — not an error.

pub mod natpmp;
pub mod upnp;

use std::net::{Ipv4Addr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::error::TorrentError;

/// How long a mapping is asked for.
const LEASE: u32 = 3600;
/// How often the mapping is asked for again when nothing holds one.
const RETRY: Duration = Duration::from_secs(60);

/// What the mapping is doing, as the window reads it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Status {
    /// The reader has not asked for a mapping.
    Off,
    /// Looking for a gateway that speaks UPnP or NAT-PMP.
    Searching,
    /// The router holds the mapping.
    Mapped {
        /// The protocol it was asked in.
        method: String,
        /// The port the router opened.
        external_port: u16,
        /// The seconds the router keeps it.
        lease_secs: u32,
    },
    /// No gateway on this network speaks either protocol.
    NoGateway,
    /// A gateway answered and refused; its words are kept.
    Refused {
        /// What the gateway said.
        message: String,
    },
}

/// The mapping this client holds, and the protocol it was asked in.
enum Held {
    Upnp(upnp::Mapping),
    NatPmp { gateway: Ipv4Addr, external: u16, lease: u32 },
}

/// The port mapping of one client.
pub struct PortMap {
    stop: mpsc::Sender<()>,
    status: Arc<Mutex<Status>>,
}

impl PortMap {
    /// Start asking the router for the port. The task refreshes the lease
    /// and gives the mapping back on stop.
    pub fn start(port: u16) -> Result<Arc<PortMap>, TorrentError> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(TorrentError::Failed(
                "the port mapping needs a runtime to run in".into(),
            ));
        }
        let (stop, receiver) = mpsc::channel(1);
        let status = Arc::new(Mutex::new(Status::Searching));
        let handle = Arc::new(PortMap {
            stop,
            status: status.clone(),
        });
        tokio::spawn(run(port, receiver, status));
        Ok(handle)
    }

    /// What the mapping is doing right now.
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or(Status::Off)
    }

    /// Give the mapping back and stop asking.
    pub fn stop(&self) {
        let _ = self.stop.try_send(());
    }
}

/// Say what the mapping is doing.
fn set(status: &Mutex<Status>, next: Status) {
    if let Ok(mut held) = status.lock() {
        *held = next;
    }
}

/// The address this machine leaves through: a UDP socket pointed at a
/// public address names the local one without sending anything.
fn local_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(address) => Some(address),
        _ => None,
    }
}

/// Ask once, UPnP first and NAT-PMP when no IGD answers.
async fn try_map(port: u16, internal: Ipv4Addr, status: &Mutex<Status>) -> Option<Held> {
    match upnp::map(port, internal, LEASE).await {
        Ok(Some(mapping)) => {
            set(
                status,
                Status::Mapped {
                    method: "upnp".into(),
                    external_port: mapping.port,
                    lease_secs: mapping.lease,
                },
            );
            return Some(Held::Upnp(mapping));
        }
        Ok(None) => {}
        Err(error) => {
            set(
                status,
                Status::Refused {
                    message: error.to_string(),
                },
            );
            return None;
        }
    }
    let Some(gateway) = natpmp::default_gateway().await else {
        set(status, Status::NoGateway);
        return None;
    };
    match natpmp::map(gateway, port, LEASE).await {
        Ok(Some((external, lease))) => {
            set(
                status,
                Status::Mapped {
                    method: "nat-pmp".into(),
                    external_port: external,
                    lease_secs: lease,
                },
            );
            Some(Held::NatPmp {
                gateway,
                external,
                lease,
            })
        }
        Ok(None) => {
            set(status, Status::NoGateway);
            None
        }
        Err(error) => {
            set(
                status,
                Status::Refused {
                    message: error.to_string(),
                },
            );
            None
        }
    }
}

/// Ask for the mapping again before its lease runs out.
async fn refresh(held: &Held, port: u16, internal: Ipv4Addr) -> Result<(), TorrentError> {
    match held {
        Held::Upnp(mapping) => upnp::refresh(mapping, internal).await,
        Held::NatPmp { gateway, lease, .. } => {
            natpmp::map(*gateway, port, *lease).await.map(|_| ())
        }
    }
}

/// Give the mapping back.
async fn release(held: &Held, port: u16) {
    match held {
        Held::Upnp(mapping) => {
            let _ = upnp::unmap(mapping).await;
        }
        Held::NatPmp {
            gateway, external, ..
        } => {
            let _ = natpmp::unmap(*gateway, port, *external).await;
        }
    }
}

/// Hold the mapping until the reader stops it.
async fn run(port: u16, mut stop: mpsc::Receiver<()>, status: Arc<Mutex<Status>>) {
    let Some(internal) = local_address() else {
        set(
            &status,
            Status::Refused {
                message: "this machine has no local address to map".into(),
            },
        );
        return;
    };
    let mut held: Option<Held> = None;
    loop {
        if held.is_none() {
            set(&status, Status::Searching);
            held = try_map(port, internal, &status).await;
        }
        let wait = match &held {
            Some(Held::Upnp(mapping)) => Duration::from_secs((mapping.lease / 2).max(30) as u64),
            Some(Held::NatPmp { lease, .. }) => Duration::from_secs((*lease / 2).max(30) as u64),
            None => RETRY,
        };
        tokio::select! {
            _ = stop.recv() => {
                if let Some(held) = &held {
                    release(held, port).await;
                }
                set(&status, Status::Off);
                break;
            }
            _ = tokio::time::sleep(wait) => {
                if let Some(held_now) = &held {
                    if refresh(held_now, port, internal).await.is_err() {
                        held = None;
                    }
                }
            }
        }
    }
}