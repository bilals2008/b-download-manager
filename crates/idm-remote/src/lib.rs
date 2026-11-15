//! The phone remote.
//!
//! A phone on the same home network can list the downloads, add a link,
//! pause, resume and remove one — after a one-time pairing, and never from
//! outside the network. The remote is a listener of its own, next to the
//! loopback bridge the browser extension talks to, and it is guarded the same
//! way: it binds to one local address, it refuses a request that did not come
//! from the local network, and it refuses a request whose `Host` header names
//! somewhere else.
//!
//! The crate knows nothing about the application. It asks for the downloads
//! through [`RemoteBackend`], and the application answers.
//!
//! ```no_run
//! use std::sync::Arc;
//! use idm_remote::{DeviceStore, RemoteConfig, RemoteServer};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let store = Arc::new(DeviceStore::new());
//! let server = RemoteServer::start(
//!     RemoteConfig { enabled: true, interface: None, port: 27600 },
//!     store,
//! )
//! .await?;
//! println!("the phone reaches the remote on {}", server.local_addr());
//! # Ok(())
//! # }
//! ```

mod backend;
mod devices;
mod http;
mod net;
mod pairing;
mod page;
mod random;
mod server;

pub use backend::{RemoteBackend, RemoteDownload};
pub use devices::{token_hash, Device, DeviceStore};
pub use net::{host_header_is_local, is_local_address};
pub use pairing::{new_pairing_code, verify_pairing, PairingCode};
pub use server::{RemoteConfig, RemoteError, RemoteServer, DEFAULT_PORT};
