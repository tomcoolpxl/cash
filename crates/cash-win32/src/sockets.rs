//! Connected sockets a child process can read and write as its standard streams.
//!
//! The standard library opens every socket overlapped (`WSA_FLAG_OVERLAPPED`), which
//! suits its own calls but not a program handed the socket as its standard output:
//! `cmd.exe`'s `WriteFile` on an overlapped socket fails with "The process tried to
//! write to a nonexistent pipe". A `/dev/tcp` descriptor the shell passes on (D7: `cat
//! <&3`, where `cat` is a process of its own) needs a plain, synchronous socket, which
//! only `WSASocketW` with no flags makes. The result is still a [`std::net::TcpStream`]
//! or [`std::net::UdpSocket`] to the standard library, whose `send` and `recv` work on
//! either kind.

use std::io;
use std::mem::size_of;
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::os::windows::io::FromRawSocket as _;
use std::time::Duration;

use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, INVALID_SOCKET, IPPROTO_TCP,
    IPPROTO_UDP, SOCK_DGRAM, SOCK_STREAM, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6, SOCKADDR_IN6_0,
    SOCKET, WSAGetLastError, WSASocketW, closesocket, connect,
};

/// A TCP connection to `address` on a synchronous socket.
///
/// # Errors
///
/// Winsock's error for the socket or the connection, as `std::io::Error` reads it
/// (`ConnectionRefused`, `TimedOut` and the rest).
pub fn connect_tcp(address: SocketAddr) -> io::Result<TcpStream> {
    let socket = connected(address, SOCK_STREAM, IPPROTO_TCP)?;
    // SAFETY: `socket` is an open socket this function owns, handed over whole.
    Ok(unsafe { TcpStream::from_raw_socket(socket as _) })
}

/// A UDP socket connected to `address`, so `send` and `recv` talk to that peer alone.
///
/// # Errors
///
/// As [`connect_tcp`].
pub fn connect_udp(address: SocketAddr) -> io::Result<UdpSocket> {
    let socket = connected(address, SOCK_DGRAM, IPPROTO_UDP)?;
    // SAFETY: as in `connect_tcp`.
    Ok(unsafe { UdpSocket::from_raw_socket(socket as _) })
}

/// Whether `socket` has bytes to read, or its peer has closed, waiting up to `timeout`.
///
/// Winsock's `select`, which every socket supports. A socket made here has no use for
/// `SO_RCVTIMEO`, which Windows honours on overlapped sockets alone.
///
/// # Errors
///
/// Winsock's error when the wait itself fails.
pub fn wait_readable(
    socket: std::os::windows::io::RawSocket,
    timeout: Duration,
) -> io::Result<bool> {
    use windows_sys::Win32::Networking::WinSock::{FD_SET, SOCKET_ERROR, TIMEVAL, select};

    // SAFETY: an FD_SET is plain data; zeroed, it holds no sockets.
    let mut readers: FD_SET = unsafe { std::mem::zeroed() };
    readers.fd_count = 1;
    readers.fd_array[0] = SOCKET::try_from(socket)
        .map_err(|_| io::Error::other("the socket handle is out of range"))?;
    let wait = TIMEVAL {
        tv_sec: i32::try_from(timeout.as_secs()).unwrap_or(i32::MAX),
        tv_usec: i32::try_from(timeout.subsec_micros()).unwrap_or(i32::MAX),
    };
    // SAFETY: `readers` holds one open socket and `wait` outlives the call; the first
    // argument is ignored on Windows.
    let ready = unsafe {
        select(
            0,
            &raw mut readers,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw const wait,
        )
    };
    if ready == SOCKET_ERROR {
        return Err(last_error());
    }
    Ok(ready > 0)
}

/// Winsock's last error as the standard library's.
fn last_error() -> io::Error {
    // SAFETY: reads this thread's last Winsock error.
    io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
}

/// A socket of `kind` and `protocol`, synchronous and inheritable, connected to
/// `address`.
fn connected(address: SocketAddr, kind: i32, protocol: i32) -> io::Result<SOCKET> {
    if !crate::net::winsock_started() {
        return Err(io::Error::other("Winsock could not be started"));
    }
    let family = if address.is_ipv4() { AF_INET } else { AF_INET6 };
    // SAFETY: plain socket creation; no protocol info, no group, no flags.
    let socket = unsafe { WSASocketW(family.into(), kind, protocol, std::ptr::null(), 0, 0) };
    if socket == INVALID_SOCKET {
        return Err(last_error());
    }
    let v4;
    let v6;
    let (pointer, length) = match address {
        SocketAddr::V4(address) => {
            v4 = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: address.port().to_be(),
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: address.ip().to_bits().to_be(),
                    },
                },
                sin_zero: [0; 8],
            };
            ((&raw const v4).cast::<SOCKADDR>(), size_of::<SOCKADDR_IN>())
        }
        SocketAddr::V6(address) => {
            v6 = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_port: address.port().to_be(),
                sin6_flowinfo: address.flowinfo(),
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 {
                        Byte: address.ip().octets(),
                    },
                },
                Anonymous: SOCKADDR_IN6_0 {
                    sin6_scope_id: address.scope_id(),
                },
            };
            (
                (&raw const v6).cast::<SOCKADDR>(),
                size_of::<SOCKADDR_IN6>(),
            )
        }
    };
    // SAFETY: `pointer` addresses a socket address of `length` bytes that outlives the
    // call, and `socket` is open.
    let connected = unsafe { connect(socket, pointer, i32::try_from(length).unwrap_or(i32::MAX)) };
    if connected != 0 {
        let error = last_error();
        // SAFETY: the socket is ours and still open.
        unsafe { closesocket(socket) };
        return Err(error);
    }
    Ok(socket)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use std::io::{Read as _, Write as _};

    use super::*;

    #[test]
    fn a_refused_connection_is_connection_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = connect_tcp(address).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
    }

    #[test]
    fn the_socket_talks_to_the_peer_and_a_child_can_write_to_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut all = Vec::new();
            stream.read_to_end(&mut all).unwrap();
            all
        });
        let mut stream = connect_tcp(address).unwrap();
        stream.write_all(b"from the shell\r\n").unwrap();
        // `cmd.exe` writes with `WriteFile`, which an overlapped socket refuses.
        let handle = {
            use std::os::windows::io::{AsRawSocket as _, BorrowedHandle};
            // SAFETY: a socket is a handle; it stays open until `stream` is dropped.
            unsafe { BorrowedHandle::borrow_raw(stream.as_raw_socket() as _) }
                .try_clone_to_owned()
                .unwrap()
        };
        let status = std::process::Command::new("cmd.exe")
            .args(["/c", "echo from a child"])
            .stdout(std::process::Stdio::from(handle))
            .status()
            .unwrap();
        assert!(status.success());
        drop(stream);
        let all = server.join().unwrap();
        assert_eq!(all, b"from the shell\r\nfrom a child\r\n");
    }

    #[test]
    fn a_quiet_socket_is_not_readable_until_bytes_or_a_close_arrive() {
        use std::os::windows::io::AsRawSocket as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = connect_tcp(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        let started = std::time::Instant::now();
        assert!(!wait_readable(stream.as_raw_socket(), Duration::from_millis(200)).unwrap());
        assert!(started.elapsed() >= Duration::from_millis(150));
        peer.write_all(b"x").unwrap();
        assert!(wait_readable(stream.as_raw_socket(), Duration::from_secs(5)).unwrap());
        let mut byte = [0u8; 1];
        (&stream).read_exact(&mut byte).unwrap();
        drop(peer);
        // The peer's close is readable too: the read then finds the end.
        assert!(wait_readable(stream.as_raw_socket(), Duration::from_secs(5)).unwrap());
    }

    #[test]
    fn a_udp_socket_sends_a_datagram() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let socket = connect_udp(receiver.local_addr().unwrap()).unwrap();
        socket.send(b"datagram").unwrap();
        let mut buffer = [0u8; 16];
        let (n, _) = receiver.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], b"datagram");
    }
}
