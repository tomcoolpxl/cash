//! The machine's TCP and UDP sockets and the processes that own them.
//!
//! One layer for `fuser PORT/tcp`, `lsof -i` and, later, `ss`, built on IP Helper's
//! `GetExtendedTcpTable` and `GetExtendedUdpTable` with the owner-PID table classes. They
//! are documented, need no elevation, and cover IPv4 and IPv6. See
//! `research/ss-evaluation.md` for the design this follows.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP_STATE_CLOSE_WAIT, MIB_TCP_STATE_CLOSING,
    MIB_TCP_STATE_ESTAB, MIB_TCP_STATE_FIN_WAIT1, MIB_TCP_STATE_FIN_WAIT2, MIB_TCP_STATE_LAST_ACK,
    MIB_TCP_STATE_LISTEN, MIB_TCP_STATE_SYN_RCVD, MIB_TCP_STATE_SYN_SENT, MIB_TCP_STATE_TIME_WAIT,
    MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID,
    TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

/// A transport protocol.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Proto {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
}

impl Proto {
    /// The lowercase name used in `PORT/tcp` specifications.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

/// A TCP connection state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TcpState {
    /// `CLOSED` (also Windows' `DELETE_TCB`).
    Closed,
    /// `LISTEN`.
    Listen,
    /// `SYN_SENT`.
    SynSent,
    /// `SYN_RECV`.
    SynReceived,
    /// `ESTABLISHED`.
    Established,
    /// `FIN_WAIT1`.
    FinWait1,
    /// `FIN_WAIT2`.
    FinWait2,
    /// `CLOSE_WAIT`.
    CloseWait,
    /// `CLOSING`.
    Closing,
    /// `LAST_ACK`.
    LastAck,
    /// `TIME_WAIT`.
    TimeWait,
}

impl TcpState {
    const fn from_mib(state: u32) -> Self {
        match state.cast_signed() {
            MIB_TCP_STATE_LISTEN => Self::Listen,
            MIB_TCP_STATE_SYN_SENT => Self::SynSent,
            MIB_TCP_STATE_SYN_RCVD => Self::SynReceived,
            MIB_TCP_STATE_ESTAB => Self::Established,
            MIB_TCP_STATE_FIN_WAIT1 => Self::FinWait1,
            MIB_TCP_STATE_FIN_WAIT2 => Self::FinWait2,
            MIB_TCP_STATE_CLOSE_WAIT => Self::CloseWait,
            MIB_TCP_STATE_CLOSING => Self::Closing,
            MIB_TCP_STATE_LAST_ACK => Self::LastAck,
            MIB_TCP_STATE_TIME_WAIT => Self::TimeWait,
            // CLOSED, DELETE_TCB and anything newer.
            _ => Self::Closed,
        }
    }

    /// The state as `lsof` prints it (`LISTEN`, `ESTABLISHED`, `TIME_WAIT`, ...).
    #[must_use]
    pub const fn lsof_name(self) -> &'static str {
        match self {
            Self::Closed => "CLOSED",
            Self::Listen => "LISTEN",
            Self::SynSent => "SYN_SENT",
            Self::SynReceived => "SYN_RECV",
            Self::Established => "ESTABLISHED",
            Self::FinWait1 => "FIN_WAIT1",
            Self::FinWait2 => "FIN_WAIT2",
            Self::CloseWait => "CLOSE_WAIT",
            Self::Closing => "CLOSING",
            Self::LastAck => "LAST_ACK",
            Self::TimeWait => "TIME_WAIT",
        }
    }
}

/// One socket and its owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Socket {
    /// TCP or UDP.
    pub proto: Proto,
    /// The local endpoint; IPv6 addresses carry their scope id.
    pub local: SocketAddr,
    /// The remote endpoint of a TCP socket that has one (not for listeners or UDP).
    pub remote: Option<SocketAddr>,
    /// The TCP state; `None` for UDP.
    pub state: Option<TcpState>,
    /// The owning process. Windows reports 0 for `TIME_WAIT` leftovers, which no process
    /// owns, and 4 (System) for kernel-owned sockets.
    pub pid: u32,
}

/// Ports are in network byte order in the low 16 bits.
const fn port(raw: u32) -> u16 {
    u16::from_be((raw & 0xffff) as u16)
}

/// IPv4 addresses are in network byte order.
const fn ipv4(raw: u32) -> Ipv4Addr {
    Ipv4Addr::from_bits(u32::from_be(raw))
}

fn v6_addr(bytes: [u8; 16], port_raw: u32, scope: u32) -> SocketAddr {
    SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::from(bytes),
        port(port_raw),
        0,
        scope,
    ))
}

/// Calls an IP Helper table function with a growing buffer, as its size can change
/// between calls. `u64` elements keep the rows suitably aligned.
fn fetch_table(call: impl Fn(*mut core::ffi::c_void, *mut u32) -> u32) -> io::Result<Vec<u64>> {
    let mut size = 0u32;
    let mut buffer: Vec<u64> = Vec::new();
    for _ in 0..8 {
        let pointer = if buffer.is_empty() {
            std::ptr::null_mut()
        } else {
            buffer.as_mut_ptr().cast()
        };
        match call(pointer, &raw mut size) {
            NO_ERROR if !buffer.is_empty() => return Ok(buffer),
            NO_ERROR | ERROR_INSUFFICIENT_BUFFER => {
                buffer = vec![0u64; (size as usize).div_ceil(8) + 64];
                size = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
            }
            code => return Err(io::Error::from_raw_os_error(code.cast_signed())),
        }
    }
    Err(io::Error::other("socket table kept growing"))
}

/// The rows of a `{ dwNumEntries, table[..] }` table fetched by [`fetch_table`].
fn rows<T: Copy>(buffer: &[u64]) -> Vec<T> {
    let bytes = buffer.len() * 8;
    if bytes < 4 {
        return Vec::new();
    }
    let base = buffer.as_ptr().cast::<u8>();
    // SAFETY: the buffer is at least four bytes and starts with the entry count.
    let count = unsafe { base.cast::<u32>().read_unaligned() } as usize;
    // The rows start at the first offset after the count that suits their alignment.
    let offset = std::mem::align_of::<T>().max(4);
    let available = bytes.saturating_sub(offset) / std::mem::size_of::<T>();
    (0..count.min(available))
        .map(|i| {
            // SAFETY: row `i` lies inside the buffer, bounded by `available`.
            let row = unsafe { base.add(offset + i * std::mem::size_of::<T>()) };
            // SAFETY: the rows are plain integer structs, valid for any bit pattern, and
            // the read makes no alignment assumption.
            unsafe { row.cast::<T>().read_unaligned() }
        })
        .collect()
}

fn tcp(family: u16) -> io::Result<Vec<Socket>> {
    let buffer = fetch_table(|table, size| {
        // SAFETY: `table` is null or a buffer of `*size` bytes; `size` is writable.
        unsafe {
            GetExtendedTcpTable(
                table,
                size,
                0,
                u32::from(family),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        }
    })?;
    Ok(if family == AF_INET {
        rows::<MIB_TCPROW_OWNER_PID>(&buffer)
            .into_iter()
            .map(|row| {
                let state = TcpState::from_mib(row.dwState);
                Socket {
                    proto: Proto::Tcp,
                    local: SocketAddr::new(
                        IpAddr::V4(ipv4(row.dwLocalAddr)),
                        port(row.dwLocalPort),
                    ),
                    remote: (state != TcpState::Listen).then(|| {
                        SocketAddr::new(IpAddr::V4(ipv4(row.dwRemoteAddr)), port(row.dwRemotePort))
                    }),
                    state: Some(state),
                    pid: row.dwOwningPid,
                }
            })
            .collect()
    } else {
        rows::<MIB_TCP6ROW_OWNER_PID>(&buffer)
            .into_iter()
            .map(|row| {
                let state = TcpState::from_mib(row.dwState);
                Socket {
                    proto: Proto::Tcp,
                    local: v6_addr(row.ucLocalAddr, row.dwLocalPort, row.dwLocalScopeId),
                    remote: (state != TcpState::Listen)
                        .then(|| v6_addr(row.ucRemoteAddr, row.dwRemotePort, row.dwRemoteScopeId)),
                    state: Some(state),
                    pid: row.dwOwningPid,
                }
            })
            .collect()
    })
}

fn udp(family: u16) -> io::Result<Vec<Socket>> {
    let buffer = fetch_table(|table, size| {
        // SAFETY: as in `tcp`.
        unsafe { GetExtendedUdpTable(table, size, 0, u32::from(family), UDP_TABLE_OWNER_PID, 0) }
    })?;
    Ok(if family == AF_INET {
        rows::<MIB_UDPROW_OWNER_PID>(&buffer)
            .into_iter()
            .map(|row| Socket {
                proto: Proto::Udp,
                local: SocketAddr::new(IpAddr::V4(ipv4(row.dwLocalAddr)), port(row.dwLocalPort)),
                remote: None,
                state: None,
                pid: row.dwOwningPid,
            })
            .collect()
    } else {
        rows::<MIB_UDP6ROW_OWNER_PID>(&buffer)
            .into_iter()
            .map(|row| Socket {
                proto: Proto::Udp,
                local: v6_addr(row.ucLocalAddr, row.dwLocalPort, row.dwLocalScopeId),
                remote: None,
                state: None,
                pid: row.dwOwningPid,
            })
            .collect()
    })
}

/// Every socket of the requested protocols and address families.
///
/// # Errors
///
/// Fails if IP Helper cannot produce a table.
pub fn sockets(protos: &[Proto], v4: bool, v6: bool) -> io::Result<Vec<Socket>> {
    let mut all = Vec::new();
    for &proto in protos {
        for (wanted, family) in [(v4, AF_INET), (v6, AF_INET6)] {
            if !wanted {
                continue;
            }
            all.extend(match proto {
                Proto::Tcp => tcp(family)?,
                Proto::Udp => udp(family)?,
            });
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listener_in_this_process_is_listed_with_its_pid() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let bound = listener.local_addr().unwrap();
        let found = sockets(&[Proto::Tcp], true, false).unwrap();
        let row = found.iter().find(|s| s.local == bound).unwrap();
        assert_eq!(row.pid, std::process::id());
        assert_eq!(row.state, Some(TcpState::Listen));
        assert_eq!(row.remote, None);
    }

    #[test]
    fn a_udp_socket_and_an_ipv6_listener_are_listed() {
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let udp_addr = udp.local_addr().unwrap();
        let v6 = std::net::TcpListener::bind("[::1]:0").unwrap();
        let v6_addr = v6.local_addr().unwrap();
        let found = sockets(&[Proto::Tcp, Proto::Udp], true, true).unwrap();
        assert!(
            found.iter().any(|s| s.proto == Proto::Udp
                && s.local == udp_addr
                && s.pid == std::process::id())
        );
        assert!(found.iter().any(|s| s.proto == Proto::Tcp
            && s.local.port() == v6_addr.port()
            && s.local.is_ipv6()));
    }

    #[test]
    fn an_established_connection_has_both_ends() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let server = listener.local_addr().unwrap();
        let client = std::net::TcpStream::connect(server).unwrap();
        let (_accepted, _) = listener.accept().unwrap();
        let client_addr = client.local_addr().unwrap();
        let found = sockets(&[Proto::Tcp], true, false).unwrap();
        let row = found.iter().find(|s| s.local == client_addr).unwrap();
        assert_eq!(row.remote, Some(server));
        assert_eq!(row.state, Some(TcpState::Established));
    }
}
