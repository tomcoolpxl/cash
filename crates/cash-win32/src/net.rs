//! The machine's TCP and UDP sockets and the processes that own them.
//!
//! One layer for `fuser PORT/tcp`, `lsof -i` and `ss`, built on IP Helper's
//! `GetExtendedTcpTable` and `GetExtendedUdpTable` with the owner-PID table classes. They
//! are documented, need no elevation, and cover IPv4 and IPv6. See
//! `research/ss-evaluation.md` for the design this follows.
//!
//! `ss` also needs what those tables leave out: bound but inactive TCP sockets
//! ([`bound_tcp_sockets`](crate::net::bound_tcp_sockets), from an undocumented export),
//! interface names for IPv6 scope ids ([`interface_name`](crate::net::interface_name)),
//! host names ([`host_names`](crate::net::host_names)), closing a connection
//! ([`close_tcp`](crate::net::close_tcp)) and a connection's extended statistics
//! ([`tcp_stats`](crate::net::tcp_stats)).

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

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
    /// The `MIB_TCP_STATE` a row for this state carries.
    const fn to_mib(self) -> i32 {
        use windows_sys::Win32::NetworkManagement::IpHelper::MIB_TCP_STATE_CLOSED;

        match self {
            Self::Closed => MIB_TCP_STATE_CLOSED,
            Self::Listen => MIB_TCP_STATE_LISTEN,
            Self::SynSent => MIB_TCP_STATE_SYN_SENT,
            Self::SynReceived => MIB_TCP_STATE_SYN_RCVD,
            Self::Established => MIB_TCP_STATE_ESTAB,
            Self::FinWait1 => MIB_TCP_STATE_FIN_WAIT1,
            Self::FinWait2 => MIB_TCP_STATE_FIN_WAIT2,
            Self::CloseWait => MIB_TCP_STATE_CLOSE_WAIT,
            Self::Closing => MIB_TCP_STATE_CLOSING,
            Self::LastAck => MIB_TCP_STATE_LAST_ACK,
            Self::TimeWait => MIB_TCP_STATE_TIME_WAIT,
        }
    }

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
    /// The module that owns the socket, from [`sockets_with_owners`]: the executable's
    /// name for an ordinary program, the service's short name for a service hosted in
    /// `svchost.exe`. `None` from [`sockets`], and when Windows cannot say.
    pub owner: Option<String>,
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
                    owner: None,
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
                    owner: None,
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
                owner: None,
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
                owner: None,
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

/// Reads the owning module's name out of a `GetOwnerModuleFrom*Entry` call.
fn owner_name(call: impl Fn(*mut core::ffi::c_void, *mut u32) -> u32) -> Option<String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::TCPIP_OWNER_MODULE_BASIC_INFO;

    let mut size = 0u32;
    if call(std::ptr::null_mut(), &raw mut size) != ERROR_INSUFFICIENT_BUFFER || size == 0 {
        return None;
    }
    // `u64` elements keep the leading pointer pair aligned.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    if call(buffer.as_mut_ptr().cast(), &raw mut size) != NO_ERROR {
        return None;
    }
    // SAFETY: on success the buffer starts with a TCPIP_OWNER_MODULE_BASIC_INFO.
    let info = unsafe {
        buffer
            .as_ptr()
            .cast::<TCPIP_OWNER_MODULE_BASIC_INFO>()
            .read()
    };
    if info.pModuleName.is_null() {
        return None;
    }
    // SAFETY: pModuleName points at a NUL-terminated string inside `buffer`, which is
    // still alive.
    let name = unsafe { widestring_at(info.pModuleName) };
    (!name.is_empty()).then_some(name)
}

/// Decodes a NUL-terminated UTF-16 string.
///
/// # Safety
///
/// `pointer` must point at a readable, NUL-terminated UTF-16 string.
pub(crate) unsafe fn widestring_at(pointer: *const u16) -> String {
    let mut length = 0;
    loop {
        // SAFETY: the caller guarantees a terminator is reachable, so every unit up to
        // it is in bounds.
        let unit = unsafe { pointer.add(length) };
        // SAFETY: as above; `unit` is in bounds and readable.
        if unsafe { unit.read() } == 0 {
            break;
        }
        length += 1;
    }
    // SAFETY: the `length` units before the terminator are readable.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, length) })
}

/// Like [`sockets`], with each socket's owning module name filled in; one extra call
/// per socket, so only for callers that print it (`ss -p`).
///
/// # Errors
///
/// Fails if IP Helper cannot produce a table.
#[expect(
    clippy::too_many_lines,
    reason = "the four owner-module row layouts differ only in field names"
)]
pub fn sockets_with_owners(protos: &[Proto], v4: bool, v6: bool) -> io::Result<Vec<Socket>> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetOwnerModuleFromTcp6Entry, GetOwnerModuleFromTcpEntry, GetOwnerModuleFromUdp6Entry,
        GetOwnerModuleFromUdpEntry, MIB_TCP6ROW_OWNER_MODULE, MIB_TCPROW_OWNER_MODULE,
        MIB_UDP6ROW_OWNER_MODULE, MIB_UDPROW_OWNER_MODULE, TCP_TABLE_OWNER_MODULE_ALL,
        TCPIP_OWNER_MODULE_INFO_BASIC, UDP_TABLE_OWNER_MODULE,
    };

    let mut all = Vec::new();
    for &proto in protos {
        for (wanted, family) in [(v4, AF_INET), (v6, AF_INET6)] {
            if !wanted {
                continue;
            }
            let buffer = fetch_table(|table, size| match proto {
                // SAFETY: `table` is null or a buffer of `*size` bytes; `size` is writable.
                Proto::Tcp => unsafe {
                    GetExtendedTcpTable(
                        table,
                        size,
                        0,
                        u32::from(family),
                        TCP_TABLE_OWNER_MODULE_ALL,
                        0,
                    )
                },
                // SAFETY: as above.
                Proto::Udp => unsafe {
                    GetExtendedUdpTable(
                        table,
                        size,
                        0,
                        u32::from(family),
                        UDP_TABLE_OWNER_MODULE,
                        0,
                    )
                },
            })?;
            match (proto, family == AF_INET) {
                (Proto::Tcp, true) => {
                    all.extend(rows::<MIB_TCPROW_OWNER_MODULE>(&buffer).iter().map(|row| {
                        let state = TcpState::from_mib(row.dwState);
                        Socket {
                            proto,
                            local: SocketAddr::new(
                                IpAddr::V4(ipv4(row.dwLocalAddr)),
                                port(row.dwLocalPort),
                            ),
                            remote: (state != TcpState::Listen).then(|| {
                                SocketAddr::new(
                                    IpAddr::V4(ipv4(row.dwRemoteAddr)),
                                    port(row.dwRemotePort),
                                )
                            }),
                            state: Some(state),
                            pid: row.dwOwningPid,
                            // SAFETY: `row` is a valid entry from the owner-module table.
                            owner: owner_name(|b, n| unsafe {
                                GetOwnerModuleFromTcpEntry(row, TCPIP_OWNER_MODULE_INFO_BASIC, b, n)
                            }),
                        }
                    }));
                }
                (Proto::Tcp, false) => {
                    all.extend(rows::<MIB_TCP6ROW_OWNER_MODULE>(&buffer).iter().map(|row| {
                        let state = TcpState::from_mib(row.dwState);
                        Socket {
                            proto,
                            local: v6_addr(row.ucLocalAddr, row.dwLocalPort, row.dwLocalScopeId),
                            remote: (state != TcpState::Listen).then(|| {
                                v6_addr(row.ucRemoteAddr, row.dwRemotePort, row.dwRemoteScopeId)
                            }),
                            state: Some(state),
                            pid: row.dwOwningPid,
                            // SAFETY: as above.
                            owner: owner_name(|b, n| unsafe {
                                GetOwnerModuleFromTcp6Entry(
                                    row,
                                    TCPIP_OWNER_MODULE_INFO_BASIC,
                                    b,
                                    n,
                                )
                            }),
                        }
                    }));
                }
                (Proto::Udp, true) => all.extend(
                    rows::<MIB_UDPROW_OWNER_MODULE>(&buffer)
                        .iter()
                        .map(|row| Socket {
                            proto,
                            local: SocketAddr::new(
                                IpAddr::V4(ipv4(row.dwLocalAddr)),
                                port(row.dwLocalPort),
                            ),
                            remote: None,
                            state: None,
                            pid: row.dwOwningPid,
                            // SAFETY: as above.
                            owner: owner_name(|b, n| unsafe {
                                GetOwnerModuleFromUdpEntry(row, TCPIP_OWNER_MODULE_INFO_BASIC, b, n)
                            }),
                        }),
                ),
                (Proto::Udp, false) => all.extend(
                    rows::<MIB_UDP6ROW_OWNER_MODULE>(&buffer)
                        .iter()
                        .map(|row| Socket {
                            proto,
                            local: v6_addr(row.ucLocalAddr, row.dwLocalPort, row.dwLocalScopeId),
                            remote: None,
                            state: None,
                            pid: row.dwOwningPid,
                            // SAFETY: as above.
                            owner: owner_name(|b, n| unsafe {
                                GetOwnerModuleFromUdp6Entry(
                                    row,
                                    TCPIP_OWNER_MODULE_INFO_BASIC,
                                    b,
                                    n,
                                )
                            }),
                        }),
                ),
            }
        }
    }
    Ok(all)
}

/// `InternalGetBoundTcpEndpointTable` and its IPv6 twin: they allocate a
/// `MIB_TCPTABLE2` (`MIB_TCP6TABLE2`) from the heap passed and store its address.
type BoundTableFn =
    unsafe extern "system" fn(*mut *mut core::ffi::c_void, *mut core::ffi::c_void, u32) -> u32;

/// The two bound-endpoint exports of `iphlpapi.dll`, looked up at run time: they are
/// undocumented (netstat's `-q` and System Informer use them), so a Windows without them
/// just has no bound sockets to show.
fn bound_table_functions() -> (Option<BoundTableFn>, Option<BoundTableFn>) {
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExA,
    };

    static FUNCTIONS: LazyLock<(Option<BoundTableFn>, Option<BoundTableFn>)> =
        LazyLock::new(|| {
            // SAFETY: the name is NUL terminated; the module stays loaded for the
            // process's life, as the functions are kept.
            let module = unsafe {
                LoadLibraryExA(
                    c"iphlpapi.dll".as_ptr().cast(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };
            if module.is_null() {
                return (None, None);
            }
            let find = |name: &core::ffi::CStr| {
                // SAFETY: the module handle is valid and the name is NUL terminated.
                let found = unsafe { GetProcAddress(module, name.as_ptr().cast()) };
                found.map(|function| {
                    // SAFETY: the signature System Informer declares for both exports.
                    unsafe { std::mem::transmute::<_, BoundTableFn>(function) }
                })
            };
            (
                find(c"InternalGetBoundTcpEndpointTable"),
                find(c"InternalGetBoundTcp6EndpointTable"),
            )
        });
    *FUNCTIONS
}

/// Calls one bound-endpoint export and copies its rows out of the table it allocated.
fn bound_rows<T: Copy>(function: BoundTableFn) -> io::Result<Vec<T>> {
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapFree};

    // SAFETY: GetProcessHeap has no preconditions.
    let heap = unsafe { GetProcessHeap() };
    let mut table: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `table` is a writable out-param and `heap` this process's heap.
    let code = unsafe { function(&raw mut table, heap, 0) };
    if code != NO_ERROR {
        return Err(io::Error::from_raw_os_error(code.cast_signed()));
    }
    if table.is_null() {
        return Ok(Vec::new());
    }
    let base = table.cast::<u8>().cast_const();
    // SAFETY: the table starts with its entry count.
    let count = unsafe { base.cast::<u32>().read_unaligned() } as usize;
    // The rows follow the count at their alignment, as in `rows`.
    let offset = std::mem::align_of::<T>().max(4);
    let found = (0..count)
        .map(|i| {
            // SAFETY: the table holds `count` rows after the count.
            let row = unsafe { base.add(offset + i * std::mem::size_of::<T>()) };
            // SAFETY: the rows are plain integer structs, valid for any bit pattern, and
            // the read makes no alignment assumption.
            unsafe { row.cast::<T>().read_unaligned() }
        })
        .collect();
    // SAFETY: the export allocated the table from `heap`, and nothing points into it now.
    unsafe { HeapFree(heap, 0, table) };
    Ok(found)
}

/// TCP sockets that are bound but neither listening nor connected: `netstat -q`'s
/// `BOUND`, and what Linux's `ss -B` calls bound-inactive.
///
/// Windows also lists the binding of a socket that went on to connect or listen; those
/// are left out by their port and process matching a row of the connection table. Each
/// socket has state `Closed`, no remote end and no owner module. The exports are
/// undocumented, so where Windows lacks them the list is empty.
///
/// # Errors
///
/// Fails if a table cannot be read.
pub fn bound_tcp_sockets(v4: bool, v6: bool) -> io::Result<Vec<Socket>> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{MIB_TCP6ROW2, MIB_TCPROW2};

    let (function4, function6) = bound_table_functions();
    let mut bound = Vec::new();
    if v4 && let Some(function) = function4 {
        bound.extend(bound_rows::<MIB_TCPROW2>(function)?.into_iter().map(|row| {
            (
                SocketAddr::new(IpAddr::V4(ipv4(row.dwLocalAddr)), port(row.dwLocalPort)),
                row.dwOwningPid,
            )
        }));
    }
    if v6 && let Some(function) = function6 {
        bound.extend(
            bound_rows::<MIB_TCP6ROW2>(function)?
                .into_iter()
                .map(|row| {
                    // SAFETY: both members of the address union are plain bytes.
                    let bytes = unsafe { row.LocalAddr.u.Byte };
                    (
                        v6_addr(bytes, row.dwLocalPort, row.dwLocalScopeId),
                        row.dwOwningPid,
                    )
                }),
        );
    }
    if bound.is_empty() {
        return Ok(Vec::new());
    }
    let active: HashSet<(u16, u32)> = sockets(&[Proto::Tcp], true, true)?
        .iter()
        .map(|s| (s.local.port(), s.pid))
        .collect();
    Ok(bound
        .into_iter()
        .filter(|(local, pid)| !active.contains(&(local.port(), *pid)))
        .map(|(local, pid)| Socket {
            proto: Proto::Tcp,
            local,
            remote: None,
            state: Some(TcpState::Closed),
            pid,
            owner: None,
        })
        .collect())
}

/// The name of a network interface by index, as `if_indextoname` gives it.
///
/// Names such as `ethernet_32769` and `loopback_0` have no spaces, unlike the alias
/// ("Wi-Fi", "vEthernet (WSL)") that Windows shows. `None` for an unknown index.
#[must_use]
pub fn interface_name(index: u32) -> Option<String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        ConvertInterfaceIndexToLuid, ConvertInterfaceLuidToNameW,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::{IF_MAX_STRING_SIZE, NET_LUID_LH};

    let mut luid = NET_LUID_LH { Value: 0 };
    // SAFETY: `luid` is a writable out-param.
    if unsafe { ConvertInterfaceIndexToLuid(index, &raw mut luid) } != NO_ERROR {
        return None;
    }
    let mut name = [0u16; IF_MAX_STRING_SIZE as usize + 1];
    // SAFETY: `luid` was filled in above, and `name` holds `name.len()` units.
    if unsafe { ConvertInterfaceLuidToNameW(&raw const luid, name.as_mut_ptr(), name.len()) }
        != NO_ERROR
    {
        return None;
    }
    let length = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    let name = String::from_utf16_lossy(&name[..length]);
    (!name.is_empty()).then_some(name)
}

/// Closes a TCP connection, as `ss -K` does: `SetTcpEntry` with `DELETE_TCB`, which
/// sends a reset. Windows allows it only to an elevated process, and only for IPv4.
///
/// # Errors
///
/// Fails with Windows' error when the connection cannot be closed (access denied when
/// not elevated, not found when it is already gone).
pub fn close_tcp(local: SocketAddrV4, remote: SocketAddrV4) -> io::Result<()> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        MIB_TCP_STATE_DELETE_TCB, MIB_TCPROW_LH, MIB_TCPROW_LH_0, SetTcpEntry,
    };

    let row = MIB_TCPROW_LH {
        Anonymous: MIB_TCPROW_LH_0 {
            State: MIB_TCP_STATE_DELETE_TCB,
        },
        dwLocalAddr: local.ip().to_bits().to_be(),
        dwLocalPort: u32::from(local.port().to_be()),
        dwRemoteAddr: remote.ip().to_bits().to_be(),
        dwRemotePort: u32::from(remote.port().to_be()),
    };
    // SAFETY: `row` is a complete MIB_TCPROW.
    match unsafe { SetTcpEntry(&raw const row) } {
        NO_ERROR => Ok(()),
        code => Err(io::Error::from_raw_os_error(code.cast_signed())),
    }
}

/// An interface's index by name, for `ss`'s `dev NAME`.
///
/// The name is the one [`interface_name`] gives (`ethernet_32769`) or the alias Windows
/// shows (`Ethernet`, `Wi-Fi`). `None` when no interface has it.
#[must_use]
pub fn interface_index(name: &str) -> Option<u32> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        ConvertInterfaceAliasToLuid, ConvertInterfaceLuidToIndex, ConvertInterfaceNameToLuidW,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;

    if name.is_empty() || name.contains('\0') {
        return None;
    }
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut luid = NET_LUID_LH { Value: 0 };
    // SAFETY: `wide` is NUL terminated and `luid` a writable out-param.
    let by_name = unsafe { ConvertInterfaceNameToLuidW(wide.as_ptr(), &raw mut luid) };
    if by_name != NO_ERROR {
        // SAFETY: as above.
        let by_alias = unsafe { ConvertInterfaceAliasToLuid(wide.as_ptr(), &raw mut luid) };
        if by_alias != NO_ERROR {
            return None;
        }
    }
    let mut index = 0u32;
    // SAFETY: `luid` was filled in above, and `index` is a writable out-param.
    let found = unsafe { ConvertInterfaceLuidToIndex(&raw const luid, &raw mut index) };
    (found == NO_ERROR && index != 0).then_some(index)
}

/// Extended statistics of one TCP connection, for `ss -i`, from
/// `GetPerTcpConnectionEStats`.
///
/// Windows keeps most of them only while collection is switched on for the connection,
/// which only an elevated process may do. Each part is `None` while its collection is
/// off, so nothing here is stale or garbage. Times are in milliseconds, windows and
/// queues in bytes.
#[derive(Clone, Copy, Default)]
pub struct TcpStats {
    /// The MSS the peer offered in its SYN (`MssRcvd`), kept for every connection.
    pub mss_received: Option<u32>,
    /// The MSS this end offered (`MssSent`), kept for every connection.
    pub mss_sent: Option<u32>,
    /// Bytes and segments sent and received.
    pub data: Option<windows_sys::Win32::NetworkManagement::IpHelper::TCP_ESTATS_DATA_ROD_v0>,
    /// The congestion window and slow-start threshold.
    pub congestion:
        Option<windows_sys::Win32::NetworkManagement::IpHelper::TCP_ESTATS_SND_CONG_ROD_v0>,
    /// Round-trip times, timeouts, the current MSS and retransmissions.
    pub path: Option<windows_sys::Win32::NetworkManagement::IpHelper::TCP_ESTATS_PATH_ROD_v0>,
    /// Data sent and not yet acknowledged, and data not yet sent.
    pub send_buffer:
        Option<windows_sys::Win32::NetworkManagement::IpHelper::TCP_ESTATS_SEND_BUFF_ROD_v0>,
    /// The window this end advertises, and data the application has not read.
    pub receive: Option<windows_sys::Win32::NetworkManagement::IpHelper::TCP_ESTATS_REC_ROD_v0>,
    /// The window the peer advertises.
    pub observed:
        Option<windows_sys::Win32::NetworkManagement::IpHelper::TCP_ESTATS_OBS_REC_ROD_v0>,
    /// Whether some part's collection was off and could not be switched on.
    pub incomplete: bool,
    /// Whether this call switched collection on for some part.
    pub switched_on: bool,
}

/// `TCP_ESTATS_SYN_OPTS_ROS_v0` with its `BOOLEAN` as a byte, which any value fits.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SynOpts {
    active_open: u8,
    mss_received: u32,
    mss_sent: u32,
}

/// A connection as the `EStats` functions name it.
enum EstatsRow {
    V4(windows_sys::Win32::NetworkManagement::IpHelper::MIB_TCPROW_LH),
    V6(windows_sys::Win32::NetworkManagement::IpHelper::MIB_TCP6ROW),
}

impl EstatsRow {
    fn of(socket: &Socket) -> Option<Self> {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            MIB_TCP6ROW, MIB_TCPROW_LH, MIB_TCPROW_LH_0,
        };
        use windows_sys::Win32::Networking::WinSock::{IN6_ADDR, IN6_ADDR_0};

        let remote = socket.remote?;
        if socket.proto != Proto::Tcp {
            return None;
        }
        let state = socket.state.map_or(MIB_TCP_STATE_ESTAB, TcpState::to_mib);
        let v6 = |address: SocketAddrV6| IN6_ADDR {
            u: IN6_ADDR_0 {
                Byte: address.ip().octets(),
            },
        };
        match (socket.local, remote) {
            (SocketAddr::V4(local), SocketAddr::V4(remote)) => Some(Self::V4(MIB_TCPROW_LH {
                Anonymous: MIB_TCPROW_LH_0 { State: state },
                dwLocalAddr: local.ip().to_bits().to_be(),
                dwLocalPort: u32::from(local.port().to_be()),
                dwRemoteAddr: remote.ip().to_bits().to_be(),
                dwRemotePort: u32::from(remote.port().to_be()),
            })),
            (SocketAddr::V6(local), SocketAddr::V6(remote)) => Some(Self::V6(MIB_TCP6ROW {
                State: state,
                LocalAddr: v6(local),
                dwLocalScopeId: local.scope_id(),
                dwLocalPort: u32::from(local.port().to_be()),
                RemoteAddr: v6(remote),
                dwRemoteScopeId: remote.scope_id(),
                dwRemotePort: u32::from(remote.port().to_be()),
            })),
            _ => None,
        }
    }

    /// `GetPerTcp(6)ConnectionEStats` into one of its three parts.
    fn get(&self, kind: i32, part: Part, buffer: *mut u8, size: usize) -> bool {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            GetPerTcp6ConnectionEStats, GetPerTcpConnectionEStats,
        };

        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let null = std::ptr::null_mut();
        let (rw, rw_size, ros, ros_size, rod, rod_size) = match part {
            Part::Rw => (buffer, size, null, 0, null, 0),
            Part::Ros => (null, 0, buffer, size, null, 0),
            Part::Rod => (null, 0, null, 0, buffer, size),
        };
        let code = match self {
            // SAFETY: `row` is a complete row, and the one non-null buffer holds `size`
            // bytes of the structure `kind` and `part` name.
            Self::V4(row) => unsafe {
                GetPerTcpConnectionEStats(
                    row, kind, rw, 0, rw_size, ros, 0, ros_size, rod, 0, rod_size,
                )
            },
            // SAFETY: as above.
            Self::V6(row) => unsafe {
                GetPerTcp6ConnectionEStats(
                    row, kind, rw, 0, rw_size, ros, 0, ros_size, rod, 0, rod_size,
                )
            },
        };
        code == NO_ERROR
    }

    /// A read-only structure of `kind`, read as plain bytes.
    fn read<T: Copy + Default>(&self, kind: i32, part: Part) -> Option<T> {
        let mut value = T::default();
        self.get(kind, part, (&raw mut value).cast::<u8>(), size_of::<T>())
            .then_some(value)
    }

    /// Whether collection of `kind` is on; `None` when Windows will not say.
    fn collecting(&self, kind: i32) -> Option<bool> {
        self.read::<u8>(kind, Part::Rw).map(|enabled| enabled != 0)
    }

    /// Switches collection of `kind` on; only an elevated process may.
    fn switch_on(&self, kind: i32) -> bool {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            SetPerTcp6ConnectionEStats, SetPerTcpConnectionEStats,
        };

        let enable = 1u8;
        let rw = (&raw const enable).cast::<u8>();
        let code = match self {
            // SAFETY: `row` is a complete row and `rw` a one-byte RW structure.
            Self::V4(row) => unsafe { SetPerTcpConnectionEStats(row, kind, rw, 0, 1, 0) },
            // SAFETY: as above.
            Self::V6(row) => unsafe { SetPerTcp6ConnectionEStats(row, kind, rw, 0, 1, 0) },
        };
        code == NO_ERROR
    }

    /// The read-only data of `kind` if its collection is on (switching it on first when
    /// `switch_on`), recording in `stats` what happened.
    fn collected<T: Copy + Default>(
        &self,
        kind: i32,
        switch_on: bool,
        stats: &mut TcpStats,
    ) -> Option<T> {
        let mut on = self.collecting(kind)?;
        if !on && switch_on && self.switch_on(kind) {
            on = self.collecting(kind) == Some(true);
            stats.switched_on |= on;
        }
        if !on {
            stats.incomplete = true;
            return None;
        }
        self.read(kind, Part::Rod)
    }
}

/// Which of an `EStats` call's three structures is asked for.
#[derive(Clone, Copy)]
enum Part {
    Rw,
    Ros,
    Rod,
}

/// The extended statistics of a TCP connection (`ss -i`), or `None` for a socket that
/// has no peer or no statistics (a listener, a TIME-WAIT leftover).
///
/// With `switch_on`, collection is switched on where it is off, which Windows allows
/// only to an elevated process and which lasts until the connection closes; counts then
/// start from that moment.
#[must_use]
pub fn tcp_stats(socket: &Socket, switch_on: bool) -> Option<TcpStats> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        TcpConnectionEstatsData, TcpConnectionEstatsObsRec, TcpConnectionEstatsPath,
        TcpConnectionEstatsRec, TcpConnectionEstatsSendBuff, TcpConnectionEstatsSndCong,
        TcpConnectionEstatsSynOpts,
    };

    let row = EstatsRow::of(socket)?;
    let syn = row.read::<SynOpts>(TcpConnectionEstatsSynOpts, Part::Ros)?;
    let mut stats = TcpStats {
        mss_received: (syn.mss_received != 0).then_some(syn.mss_received),
        mss_sent: (syn.mss_sent != 0).then_some(syn.mss_sent),
        ..TcpStats::default()
    };
    stats.data = row.collected(TcpConnectionEstatsData, switch_on, &mut stats);
    stats.congestion = row.collected(TcpConnectionEstatsSndCong, switch_on, &mut stats);
    stats.path = row.collected(TcpConnectionEstatsPath, switch_on, &mut stats);
    stats.send_buffer = row.collected(TcpConnectionEstatsSendBuff, switch_on, &mut stats);
    stats.receive = row.collected(TcpConnectionEstatsRec, switch_on, &mut stats);
    stats.observed = row.collected(TcpConnectionEstatsObsRec, switch_on, &mut stats);
    Some(stats)
}

/// A TCP connection's Recv-Q and Send-Q, as `ss` shows them, when Windows collects
/// them for it (an elevated `ss -i` switched collection on).
///
/// Recv-Q is the data received and not yet read by the application (`CurAppRQueue`);
/// Send-Q, as in Linux, the data not yet acknowledged by the peer: sent and
/// unacknowledged (`CurRetxQueue`) plus not yet sent (`CurAppWQueue`). Each is `None`
/// while its collection is off.
#[must_use]
pub fn tcp_queues(socket: &Socket) -> (Option<usize>, Option<usize>) {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        TCP_ESTATS_REC_ROD_v0, TCP_ESTATS_SEND_BUFF_ROD_v0, TcpConnectionEstatsRec,
        TcpConnectionEstatsSendBuff,
    };

    let Some(row) = EstatsRow::of(socket) else {
        return (None, None);
    };
    let mut ignored = TcpStats::default();
    let receive: Option<TCP_ESTATS_REC_ROD_v0> =
        row.collected(TcpConnectionEstatsRec, false, &mut ignored);
    let send: Option<TCP_ESTATS_SEND_BUFF_ROD_v0> =
        row.collected(TcpConnectionEstatsSendBuff, false, &mut ignored);
    (
        receive.map(|r| r.CurAppRQueue),
        send.map(|s| s.CurRetxQueue + s.CurAppWQueue),
    )
}

/// Whether Winsock is started, which `GetNameInfoW` and `crate::sockets` need. Started
/// once and left running, as the standard library does.
pub(crate) fn winsock_started() -> bool {
    use windows_sys::Win32::Networking::WinSock::{WSADATA, WSAStartup};

    static STARTED: LazyLock<bool> = LazyLock::new(|| {
        // SAFETY: WSADATA is plain data, valid zeroed.
        let mut data: WSADATA = unsafe { std::mem::zeroed() };
        // SAFETY: `data` is a writable WSADATA.
        unsafe { WSAStartup(0x0202, &raw mut data) == 0 }
    });
    *STARTED
}

/// The host name of one address by reverse lookup (`GetNameInfoW` with `NI_NAMEREQD`),
/// or `None` when there is none.
fn host_name(address: IpAddr) -> Option<String> {
    use windows_sys::Win32::Networking::WinSock::{
        GetNameInfoW, IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, NI_MAXHOST, NI_NAMEREQD, SOCKADDR,
        SOCKADDR_IN, SOCKADDR_IN6, SOCKADDR_IN6_0,
    };

    if !winsock_started() {
        return None;
    }
    let v4;
    let v6;
    let (pointer, length) = match address {
        IpAddr::V4(ip) => {
            v4 = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: ip.to_bits().to_be(),
                    },
                },
                sin_zero: [0; 8],
            };
            ((&raw const v4).cast::<SOCKADDR>(), size_of::<SOCKADDR_IN>())
        }
        IpAddr::V6(ip) => {
            v6 = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_port: 0,
                sin6_flowinfo: 0,
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 { Byte: ip.octets() },
                },
                Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 0 },
            };
            (
                (&raw const v6).cast::<SOCKADDR>(),
                size_of::<SOCKADDR_IN6>(),
            )
        }
    };
    let mut host = [0u16; NI_MAXHOST as usize];
    // SAFETY: `pointer` addresses a socket address of `length` bytes that lives until
    // the call returns; `host` holds `host.len()` units, and no service is asked for.
    let code = unsafe {
        GetNameInfoW(
            pointer,
            i32::try_from(length).unwrap_or(i32::MAX),
            host.as_mut_ptr(),
            u32::try_from(host.len()).unwrap_or(u32::MAX),
            std::ptr::null_mut(),
            0,
            NI_NAMEREQD.cast_signed(),
        )
    };
    if code != 0 {
        return None;
    }
    let length = host
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(host.len());
    let name = String::from_utf16_lossy(&host[..length]);
    (!name.is_empty()).then_some(name)
}

/// Reverse lookups already answered in this process, including those that found no
/// name. A lookup that outlived its caller's wait still lands here, for the next call.
static HOST_NAMES: LazyLock<Mutex<HashMap<IpAddr, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// How many reverse lookups run at once.
const LOOKUP_THREADS: usize = 16;

/// The host names of `addresses`, for `ss -r`, waiting at most `limit` in all.
///
/// The lookups run concurrently, so one slow DNS server cannot stall the caller.
/// Addresses without a name, or whose lookup is still running when the time is up, are
/// missing from the map; answers are cached for the process's life.
#[must_use]
pub fn host_names(addresses: &[IpAddr], limit: Duration) -> HashMap<IpAddr, String> {
    let deadline = Instant::now() + limit;
    let cache = || HOST_NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    let pending: Vec<IpAddr> = {
        let known = cache();
        let mut seen = HashSet::new();
        addresses
            .iter()
            .copied()
            .filter(|address| !known.contains_key(address) && seen.insert(*address))
            .collect()
    };
    if !pending.is_empty() {
        let total = pending.len();
        let queue = Arc::new(Mutex::new(pending));
        let (done, finished) = mpsc::channel();
        for _ in 0..total.min(LOOKUP_THREADS) {
            let queue = Arc::clone(&queue);
            let done = done.clone();
            // Detached: a lookup still running at the deadline finishes into the cache.
            let _ = std::thread::Builder::new()
                .name("ss-resolve".to_owned())
                .spawn(move || {
                    loop {
                        let next = queue.lock().unwrap_or_else(PoisonError::into_inner).pop();
                        let Some(address) = next else {
                            break;
                        };
                        let name = host_name(address);
                        HOST_NAMES
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(address, name);
                        let _ = done.send(());
                    }
                });
        }
        drop(done);
        for _ in 0..total {
            let left = deadline.saturating_duration_since(Instant::now());
            if finished.recv_timeout(left).is_err() {
                break;
            }
        }
    }
    let known = cache();
    addresses
        .iter()
        .filter_map(|address| {
            known
                .get(address)
                .cloned()
                .flatten()
                .map(|name| (*address, name))
        })
        .collect()
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
    fn owners_name_this_process_executable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let bound = listener.local_addr().unwrap();
        let found = sockets_with_owners(&[Proto::Tcp], true, false).unwrap();
        let row = found.iter().find(|s| s.local == bound).unwrap();
        let exe = std::env::current_exe().unwrap();
        let name = exe
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        assert_eq!(
            row.owner.as_deref().map(str::to_ascii_lowercase),
            Some(name)
        );
        assert_eq!(row.pid, std::process::id());
    }

    /// A TCP socket bound to 127.0.0.1 or `::1` on a free port, neither listening nor
    /// connected, closed on drop. The standard library cannot make one.
    struct BoundOnly(windows_sys::Win32::Networking::WinSock::SOCKET, u16);

    impl BoundOnly {
        fn new(v6: bool) -> Self {
            use windows_sys::Win32::Networking::WinSock::{
                IN6_ADDR, IN6_ADDR_0, INVALID_SOCKET, IPPROTO_TCP, SOCK_STREAM, SOCKADDR,
                SOCKADDR_IN6, SOCKADDR_IN6_0, bind, getsockname, socket,
            };
            assert!(winsock_started());
            let family = if v6 { AF_INET6 } else { AF_INET };
            // SAFETY: plain socket creation.
            let handle = unsafe { socket(family.into(), SOCK_STREAM, IPPROTO_TCP) };
            assert_ne!(handle, INVALID_SOCKET);
            // A SOCKADDR_IN6 has room for either family; a SOCKADDR_IN's address sits
            // where sin6_flowinfo is.
            let mut address = SOCKADDR_IN6 {
                sin6_family: family,
                sin6_port: 0,
                sin6_flowinfo: if v6 {
                    0
                } else {
                    Ipv4Addr::LOCALHOST.to_bits().to_be()
                },
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 {
                        Byte: if v6 {
                            Ipv6Addr::LOCALHOST.octets()
                        } else {
                            [0; 16]
                        },
                    },
                },
                Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 0 },
            };
            let full = i32::try_from(size_of::<SOCKADDR_IN6>()).unwrap();
            let length = if v6 { full } else { 16 };
            // SAFETY: `address` holds a socket address of `length` bytes.
            let bound = unsafe { bind(handle, (&raw const address).cast::<SOCKADDR>(), length) };
            assert_eq!(bound, 0);
            let mut length = full;
            // SAFETY: `address` has room for `length` bytes, and `length` is writable.
            let named = unsafe {
                getsockname(
                    handle,
                    (&raw mut address).cast::<SOCKADDR>(),
                    &raw mut length,
                )
            };
            assert_eq!(named, 0);
            Self(handle, u16::from_be(address.sin6_port))
        }
    }

    impl Drop for BoundOnly {
        fn drop(&mut self) {
            // SAFETY: the socket is ours and still open.
            unsafe { windows_sys::Win32::Networking::WinSock::closesocket(self.0) };
        }
    }

    #[test]
    fn a_bound_socket_is_listed_and_active_ones_are_not() {
        let bound = BoundOnly::new(false);
        let bound6 = BoundOnly::new(true);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let found = bound_tcp_sockets(true, true).unwrap();
        for (port, ip) in [
            (bound.1, IpAddr::V4(Ipv4Addr::LOCALHOST)),
            (bound6.1, IpAddr::V6(Ipv6Addr::LOCALHOST)),
        ] {
            let row = found
                .iter()
                .find(|s| s.local.port() == port && s.local.is_ipv6() == ip.is_ipv6());
            assert!(row.is_some(), "port {port} not in {found:?}");
            let row = row.unwrap();
            assert_eq!(row.local.ip(), ip);
            assert_eq!(row.pid, std::process::id());
            assert_eq!(row.state, Some(TcpState::Closed));
        }
        let listening = listener.local_addr().unwrap().port();
        assert!(!found.iter().any(|s| s.local.port() == listening));
        // Asking for one family leaves the other out.
        let only4 = bound_tcp_sockets(true, false).unwrap();
        assert!(only4.iter().all(|s| s.local.is_ipv4()));
    }

    #[test]
    fn the_loopback_interface_has_a_name_without_spaces() {
        // Index 1 is the loopback pseudo-interface on every Windows.
        let name = interface_name(1).unwrap();
        assert!(!name.is_empty() && !name.contains(' '), "{name:?}");
        assert_eq!(interface_name(u32::MAX), None);
    }

    #[test]
    fn an_interface_is_found_by_its_name() {
        let name = interface_name(1).unwrap();
        assert_eq!(interface_index(&name), Some(1));
        assert_eq!(interface_index("no-such-interface"), None);
        assert_eq!(interface_index(""), None);
    }

    /// A loopback connection of this process, with `bytes` sent from the client to the
    /// server and read there.
    fn loopback_connection(bytes: usize) -> (std::net::TcpStream, std::net::TcpStream) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut accepted, _) = listener.accept().unwrap();
        client.write_all(&vec![b'x'; bytes]).unwrap();
        let mut read = vec![0u8; bytes];
        accepted.read_exact(&mut read).unwrap();
        (client, accepted)
    }

    fn row_of(stream: &std::net::TcpStream) -> Socket {
        let local = stream.local_addr().unwrap();
        let found = sockets(&[Proto::Tcp], true, false).unwrap();
        let row = found.into_iter().find(|s| s.local == local);
        assert!(row.is_some(), "{local} not listed");
        row.unwrap()
    }

    #[test]
    fn syn_options_are_kept_and_collected_parts_only_when_on() {
        let (client, _accepted) = loopback_connection(100_000);
        let socket = row_of(&client);
        let stats = tcp_stats(&socket, false).unwrap();
        assert!(stats.mss_received.is_some_and(|mss| mss >= 536));
        assert!(stats.mss_sent.is_some_and(|mss| mss >= 536));
        assert!(!stats.switched_on);
        // A new connection collects nothing until someone switches it on.
        assert!(stats.incomplete);
        assert!(stats.data.is_none() && stats.path.is_none());
        assert_eq!(tcp_queues(&socket), (None, None));
        // A listener has no statistics.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let listening = sockets(&[Proto::Tcp], true, false)
            .unwrap()
            .into_iter()
            .find(|s| s.local == address);
        assert!(listening.is_some_and(|s| tcp_stats(&s, false).is_none()));
    }

    #[test]
    fn switching_collection_on_needs_elevation() {
        use std::io::Write;

        let (mut client, _accepted) = loopback_connection(1000);
        let socket = row_of(&client);
        let first = tcp_stats(&socket, true).unwrap();
        if crate::process::current_process_is_elevated() != Some(true) {
            assert!(!first.switched_on && first.incomplete);
            assert!(first.data.is_none());
            return;
        }
        assert!(first.switched_on && !first.incomplete);
        client.write_all(&[b'y'; 5000]).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let later = tcp_stats(&socket, false).unwrap();
        assert!(!later.switched_on && !later.incomplete);
        assert!(later.data.is_some_and(|d| d.DataBytesOut >= 5000));
        assert!(tcp_queues(&socket).0.is_some());
    }

    #[test]
    fn host_names_wait_no_longer_than_the_limit() {
        // TEST-NET-1 has no reverse name; with no time at all, nothing is waited for.
        let address = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let started = Instant::now();
        let names = host_names(&[address], Duration::ZERO);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(names.is_empty());
    }

    #[test]
    fn closing_a_connection_needs_elevation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let server = listener.local_addr().unwrap();
        let client = std::net::TcpStream::connect(server).unwrap();
        let (_accepted, _) = listener.accept().unwrap();
        let ends = match (client.local_addr().unwrap(), server) {
            (SocketAddr::V4(local), SocketAddr::V4(remote)) => Some((local, remote)),
            _ => None,
        };
        let (local, remote) = ends.unwrap();
        let closed = close_tcp(local, remote);
        if crate::process::current_process_is_elevated() == Some(true) {
            closed.unwrap();
            let mut byte = [0u8; 1];
            assert!(std::io::Read::read(&mut &client, &mut byte).is_err());
        } else {
            assert!(closed.is_err());
        }
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
