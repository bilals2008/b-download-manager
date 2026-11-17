//! Holding a socket to one adapter, so that it cannot leave through another.
//!
//! The socket is opened, bound and given its interface *before* it connects:
//! a connection that is not bound may already have taken the normal route by
//! the time the address is known.

use std::io;
use std::net::SocketAddr;

use super::Binding;

/// A socket ready to connect to `dst`, held to `binding`.
///
/// The socket is bound to the adapter's own address and told which interface
/// it belongs to. On Windows the address alone already holds the route (the
/// strong host model: a packet with that source address leaves only through
/// that adapter), and the interface option makes it explicit. On Linux the
/// device itself is bound, because there the route is chosen by the
/// destination and an address alone would not hold.
pub fn open(dst: SocketAddr, binding: &Binding) -> io::Result<tokio::net::TcpSocket> {
    #[cfg(windows)]
    {
        use socket2::{Domain, Protocol, Socket, Type};
        use std::os::windows::io::{FromRawSocket, IntoRawSocket};
        use windows_sys::Win32::Networking::WinSock::{setsockopt, IPPROTO_IP, IPPROTO_IPV6};

        let local = binding
            .local_for(dst.ip())
            .ok_or_else(|| io::Error::other("the chosen adapter has no address of that family"))?;
        let socket = Socket::new(Domain::for_address(dst), Type::STREAM, Some(Protocol::TCP))?;
        socket.bind(&SocketAddr::new(local, 0).into())?;
        socket.set_nonblocking(true)?;
        let handle = socket.into_raw_socket();
        if let Some((option, bytes)) = super::interface_option(dst.ip(), binding.index) {
            let level = match dst.ip() {
                std::net::IpAddr::V4(_) => IPPROTO_IP,
                std::net::IpAddr::V6(_) => IPPROTO_IPV6,
            };
            let result = unsafe {
                setsockopt(
                    handle as usize,
                    level,
                    option,
                    bytes.as_ptr(),
                    bytes.len() as i32,
                )
            };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let stream = unsafe { std::net::TcpStream::from_raw_socket(handle) };
        stream.set_nonblocking(true)?;
        Ok(tokio::net::TcpSocket::from_std_stream(stream))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use socket2::{Domain, Protocol, Socket, Type};
        use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};

        let socket = Socket::new(Domain::for_address(dst), Type::STREAM, Some(Protocol::TCP))?;
        // Linux binds the device by its own name; the kernel needs the caller
        // to be allowed to do it, and a refusal is said plainly rather than
        // letting the connection leave through a route nobody chose.
        let name = binding.name.as_bytes();
        let result = unsafe {
            libc::setsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_BINDTODEVICE,
                name.as_ptr() as *const libc::c_void,
                name.len() as libc::socklen_t,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        socket.set_nonblocking(true)?;
        let fd = socket.into_raw_fd();
        let stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
        stream.set_nonblocking(true)?;
        Ok(tokio::net::TcpSocket::from_std_stream(stream))
    }
    #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
    {
        let _ = (dst, binding);
        Err(io::Error::other(
            "holding a socket to an adapter is not supported on this system",
        ))
    }
}
