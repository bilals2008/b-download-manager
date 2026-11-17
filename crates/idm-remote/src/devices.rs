//! The devices that are allowed to talk to the remote.
//!
//! A phone gets a token when it is paired, and the application keeps only the
//! hash of that token, so a settings file that is read by someone else does
//! not hand them the remote. A device is revoked by name, and the token stops
//! working at once.

use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::random;

/// How many bytes a token is made of.
const TOKEN_BYTES: usize = 32;

/// How many bytes a device id is made of.
const ID_BYTES: usize = 16;

/// One paired device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub paired_at: SystemTime,
}

/// The paired devices and the tokens they hold.
///
/// The lock is inside, so the same store can be shared by the application and
/// the listener without either of them owning it.
#[derive(Debug, Default)]
pub struct DeviceStore {
    devices: Mutex<Vec<Device>>,
}

impl DeviceStore {
    /// A store with no devices in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pair a device.
    ///
    /// The token is handed back once, in the clear, and only its hash is kept.
    pub fn add(&self, name: &str) -> (Device, String) {
        let token = random::hex(TOKEN_BYTES).unwrap_or_default();
        let device = Device {
            id: random::hex(ID_BYTES).unwrap_or_default(),
            name: name.trim().to_string(),
            token_hash: token_hash(&token),
            paired_at: SystemTime::now(),
        };
        self.devices().push(device.clone());
        (device, token)
    }

    /// Forget a device. Its token stops working at once.
    pub fn revoke(&self, id: &str) -> bool {
        let mut devices = self.devices();
        let before = devices.len();
        devices.retain(|device| device.id != id);
        devices.len() != before
    }

    /// The paired devices.
    pub fn list(&self) -> Vec<Device> {
        self.devices().clone()
    }

    /// Whether a token belongs to a device that has not been revoked.
    pub fn check(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        let hash = token_hash(token);
        self.devices()
            .iter()
            .any(|device| device.token_hash.as_bytes().ct_eq(hash.as_bytes()).into())
    }

    /// The devices, with the lock held.
    fn devices(&self) -> MutexGuard<'_, Vec<Device>> {
        self.devices
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Put the devices back, read from the file they are kept in.
    pub fn replace_with(&self, devices: Vec<Device>) {
        *self.devices() = devices;
    }
}

/// The hash a token is stored as.
pub fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        out.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_token_passes() {
        let store = DeviceStore::new();
        let (_, token) = store.add("phone");
        assert!(store.check(&token));
    }

    #[test]
    fn an_unknown_token_fails() {
        let store = DeviceStore::new();
        store.add("phone");
        assert!(!store.check("not the token"));
        assert!(!store.check(""));
    }

    #[test]
    fn a_revoked_device_fails() {
        let store = DeviceStore::new();
        let (device, token) = store.add("phone");
        assert!(store.revoke(&device.id));
        assert!(!store.check(&token));
        assert!(store.list().is_empty());
    }

    #[test]
    fn the_list_names_the_devices() {
        let store = DeviceStore::new();
        store.add("  phone  ");
        store.add("tablet");
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "phone");
        assert_eq!(list[1].name, "tablet");
    }

    #[test]
    fn the_hash_is_a_sha256_and_the_token_is_not_in_it() {
        let hash = token_hash("secret");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(hash, token_hash("other"));
        assert!(!hash.contains("secret"));
    }
}
