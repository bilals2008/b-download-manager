//! The computer's network adapters, as Windows knows them.
//!
//! `GetAdaptersAddresses` is the call the system itself uses: it gives every
//! adapter with its addresses, its interface index and whether it is up. The
//! adapter's name (`AdapterName`) is its GUID, and that is the identity the
//! settings file keeps: the display name changes when the reader renames the
//! connection, and the interface index does not have to be the same after a
//! restart.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_INET, SOCKADDR_STORAGE, SOCKET_ADDRESS,
};

use super::Adapter;

/// The adapters of this computer, in the system's order.
pub(super) fn adapters() -> Vec<Adapter> {
    unsafe {
        // The first call only asks how much room the list needs.
        let mut size: u32 = 0;
        GetAdaptersAddresses(
            AF_UNSPEC as u32,
            flags(),
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut size,
        );
        if size == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u8; size as usize];
        let list = GetAdaptersAddresses(
            AF_UNSPEC as u32,
            flags(),
            std::ptr::null(),
            buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
            &mut size,
        );
        if list != windows_sys::Win32::Foundation::ERROR_SUCCESS {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut current = buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        while !current.is_null() {
            let adapter = &*current;
            out.push(read(adapter));
            current = adapter.Next;
        }
        out
    }
}

/// The flags the list is read with: the addresses that can reach another
/// computer are enough, and the rest is noise.
fn flags() -> u32 {
    GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER
}

/// One adapter, as the system describes it.
unsafe fn read(adapter: &IP_ADAPTER_ADDRESSES_LH) -> Adapter {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    let mut local_only = Vec::new();
    let mut address = adapter.FirstUnicastAddress;
    while !address.is_null() {
        let entry = &*address;
        if let Some(ip) = address_of(&entry.Address) {
            match ip {
                IpAddr::V4(ip) if ip.is_link_local() => local_only.push(IpAddr::V4(ip)),
                IpAddr::V4(ip) => v4.push(ip),
                IpAddr::V6(ip) if ip.is_unicast_link_local() => local_only.push(IpAddr::V6(ip)),
                IpAddr::V6(ip) => v6.push(ip),
            }
        }
        address = entry.Next;
    }
    Adapter {
        id: adapter_name(adapter.AdapterName),
        name: wide_to_string(adapter.FriendlyName),
        index: adapter.Anonymous1.Anonymous.IfIndex,
        up: adapter.OperStatus == windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp,
        v4,
        v6,
        local_only,
    }
}

/// The address a socket address carries, if it is one.
unsafe fn address_of(address: &SOCKET_ADDRESS) -> Option<IpAddr> {
    if address.lpSockaddr.is_null() {
        return None;
    }
    let storage = &*(address.lpSockaddr as *const SOCKADDR_STORAGE);
    let inet = &*(address.lpSockaddr as *const SOCKADDR_INET);
    match storage.ss_family {
        AF_INET => {
            let raw = inet.Ipv4.sin_addr.S_un.S_un_b;
            Some(IpAddr::V4(Ipv4Addr::new(
                raw.s_b1, raw.s_b2, raw.s_b3, raw.s_b4,
            )))
        }
        AF_INET6 => {
            let bytes = inet.Ipv6.sin6_addr.u.Byte;
            Some(IpAddr::V6(Ipv6Addr::from(bytes)))
        }
        _ => None,
    }
}

/// The adapter's name as the system gives it: its GUID, in plain text.
unsafe fn adapter_name(value: *const u8) -> String {
    if value.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    while *value.add(length) != 0 {
        length += 1;
    }
    let slice = std::slice::from_raw_parts(value, length);
    String::from_utf8_lossy(slice).to_string()
}

/// A string the system gave as UTF-16.
unsafe fn wide_to_string(value: *const u16) -> String {
    if value.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    while *value.add(length) != 0 {
        length += 1;
    }
    let slice = std::slice::from_raw_parts(value, length);
    String::from_utf16_lossy(slice)
}
