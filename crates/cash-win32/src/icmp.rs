//! ICMP echo through the IP Helper API, for `ping`.
//!
//! Windows gives unprivileged programs no raw sockets, so `IcmpSendEcho2` and
//! `Icmp6SendEcho2` are the documented way to send an echo request. They are used
//! asynchronously, with an event, so that a wait of several seconds can be abandoned the
//! moment the user presses Ctrl-C; the round trip is timed here, since the API reports
//! it in whole milliseconds and `ping` prints microseconds.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    ICMP_ECHO_REPLY, ICMPV6_ECHO_REPLY_LH, IP_DEST_HOST_UNREACHABLE, IP_DEST_NET_UNREACHABLE,
    IP_DEST_PORT_UNREACHABLE, IP_OPTION_INFORMATION, IP_REQ_TIMED_OUT, IP_SUCCESS,
    IP_TTL_EXPIRED_TRANSIT, Icmp6CreateFile, Icmp6ParseReplies, Icmp6SendEcho2, IcmpCloseHandle,
    IcmpCreateFile, IcmpParseReplies, IcmpSendEcho2,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET6, SOCKADDR_IN6};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

/// How often, in milliseconds, a pending echo checks whether it has been cancelled.
const CANCEL_POLL_MS: u32 = 50;

/// Room in the reply buffer beyond the reply structure and the echoed data: an ICMP
/// error's 8 bytes and the `IO_STATUS_BLOCK` the API asks for, with margin.
const REPLY_SLACK: usize = 8 + 64;

/// What came back for one echo request.
#[derive(Debug)]
pub enum Outcome {
    /// An echo reply.
    Reply {
        /// Who answered.
        from: IpAddr,
        /// The reply's time to live; Windows reports none for IPv6.
        ttl: Option<u8>,
        /// Bytes of ICMP data in the reply, header included, as `ping` counts them.
        bytes: usize,
        /// The round trip, timed here.
        time: Duration,
    },
    /// No answer within the timeout.
    TimedOut,
    /// An ICMP error from a router on the way, such as "Destination Host Unreachable".
    Error {
        /// Who reported it.
        from: IpAddr,
        /// iputils' wording for it.
        message: &'static str,
    },
    /// The cancellation flag was raised while waiting.
    Cancelled,
}

/// An open ICMP handle for one address family.
///
/// Not an [`OwnedHandle`]: an ICMP handle is closed with `IcmpCloseHandle`, which
/// `CloseHandle` is not documented to stand in for.
pub struct Pinger {
    handle: HANDLE,
    v6: bool,
}

// SAFETY: an ICMP handle is a kernel handle usable from any thread.
unsafe impl Send for Pinger {}

/// One request in flight: its reply buffer and completion event.
struct Request {
    reply: Box<[u8]>,
    event: OwnedHandle,
}

impl Pinger {
    /// Opens a handle for IPv6 when `v6`, IPv4 otherwise.
    pub fn new(v6: bool) -> io::Result<Self> {
        let handle = if v6 {
            // SAFETY: a plain call with no arguments.
            unsafe { Icmp6CreateFile() }
        } else {
            // SAFETY: as above.
            unsafe { IcmpCreateFile() }
        };
        if handle.is_null() || handle as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle, v6 })
    }

    /// Sends one echo request with `size` data bytes to `dest` and waits for the answer,
    /// up to `timeout`, or until `cancelled` is raised.
    pub fn echo(
        &self,
        dest: IpAddr,
        size: u16,
        ttl: Option<u8>,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> io::Result<Outcome> {
        let started = Instant::now();
        let request = self.send(dest, size, ttl, timeout)?;
        if !wait(&request, cancelled) {
            // The request may still complete into the buffer, which must stay valid.
            // The process is about to report and exit, so leaking it costs nothing.
            let Request { reply, event } = request;
            drop(event);
            Box::leak(reply);
            return Ok(Outcome::Cancelled);
        }
        let time = started.elapsed();
        let Request { mut reply, event } = request;
        drop(event);
        Ok(self.parse(&mut reply, size, time))
    }

    /// Starts the request; its completion signals the returned event.
    fn send(
        &self,
        dest: IpAddr,
        size: u16,
        ttl: Option<u8>,
        timeout: Duration,
    ) -> io::Result<Request> {
        let data: Vec<u8> = (0..size).map(|i| i.to_le_bytes()[0]).collect();
        let reply_len = size_of::<ICMP_ECHO_REPLY>().max(size_of::<ICMPV6_ECHO_REPLY_LH>())
            + usize::from(size)
            + REPLY_SLACK;
        let reply_size = u32::try_from(reply_len).unwrap_or(u32::MAX);
        let mut reply = vec![0u8; reply_len].into_boxed_slice();
        let options = IP_OPTION_INFORMATION {
            Ttl: ttl.unwrap_or(128),
            ..Default::default()
        };
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);

        // SAFETY: a manual-reset, initially unsignalled, unnamed event.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        // SAFETY: just returned, and nothing has run since.
        let event = unsafe { crate::handle::from_null(event)? };

        let sent = match (self.v6, dest) {
            (true, IpAddr::V6(target)) => {
                let source = sockaddr_in6(Ipv6Addr::UNSPECIFIED);
                let destination = sockaddr_in6(target);
                // SAFETY: every pointer is valid for the call; the reply buffer and the
                // event outlive the request (the buffer is leaked if a wait is abandoned).
                unsafe {
                    Icmp6SendEcho2(
                        self.handle,
                        event.as_raw_handle(),
                        None,
                        std::ptr::null(),
                        &raw const source,
                        &raw const destination,
                        data.as_ptr().cast(),
                        size,
                        &raw const options,
                        reply.as_mut_ptr().cast(),
                        reply_size,
                        timeout_ms,
                    )
                }
            }
            (false, IpAddr::V4(target)) => {
                // The address in network byte order, as the API takes it.
                let address = u32::from_ne_bytes(target.octets());
                // SAFETY: as above.
                unsafe {
                    IcmpSendEcho2(
                        self.handle,
                        event.as_raw_handle(),
                        None,
                        std::ptr::null(),
                        address,
                        data.as_ptr().cast(),
                        size,
                        &raw const options,
                        reply.as_mut_ptr().cast(),
                        reply_size,
                        timeout_ms,
                    )
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "address family does not match the handle",
                ));
            }
        };
        if sent == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != i32::try_from(ERROR_IO_PENDING).ok() {
                return Err(error);
            }
        }
        Ok(Request { reply, event })
    }

    /// Reads the completed reply.
    fn parse(&self, reply: &mut [u8], size: u16, time: Duration) -> Outcome {
        let reply_size = u32::try_from(reply.len()).unwrap_or(u32::MAX);
        if self.v6 {
            // SAFETY: the request completed into `reply`.
            let count = unsafe { Icmp6ParseReplies(reply.as_mut_ptr().cast(), reply_size) };
            if count == 0 {
                return Outcome::TimedOut;
            }
            // SAFETY: a completed reply begins with an ICMPV6_ECHO_REPLY_LH; the buffer
            // is bytes, so the read is unaligned.
            let parsed: ICMPV6_ECHO_REPLY_LH =
                unsafe { std::ptr::read_unaligned(reply.as_ptr().cast()) };
            let from = IpAddr::V6(Ipv6Addr::from(parsed.Address.sin6_addr.map(u16::from_be)));
            classify(parsed.Status, from, None, usize::from(size) + 8, time)
        } else {
            // SAFETY: the request completed into `reply`.
            let count = unsafe { IcmpParseReplies(reply.as_mut_ptr().cast(), reply_size) };
            if count == 0 {
                return Outcome::TimedOut;
            }
            // SAFETY: a completed reply begins with an ICMP_ECHO_REPLY.
            let parsed: ICMP_ECHO_REPLY =
                unsafe { std::ptr::read_unaligned(reply.as_ptr().cast()) };
            let from = IpAddr::V4(Ipv4Addr::from(parsed.Address.to_ne_bytes()));
            let bytes = usize::from(parsed.DataSize) + 8;
            classify(parsed.Status, from, Some(parsed.Options.Ttl), bytes, time)
        }
    }
}

impl Drop for Pinger {
    fn drop(&mut self) {
        // SAFETY: closing the handle this value owns, once.
        unsafe { IcmpCloseHandle(self.handle) };
    }
}

/// Waits for `request` to complete; `false` if `cancelled` was raised first.
fn wait(request: &Request, cancelled: &AtomicBool) -> bool {
    loop {
        // SAFETY: waiting on the event the request owns.
        if unsafe { WaitForSingleObject(request.event.as_raw_handle(), CANCEL_POLL_MS) }
            == WAIT_OBJECT_0
        {
            return true;
        }
        if cancelled.load(Ordering::SeqCst) {
            return false;
        }
    }
}

const fn classify(
    status: u32,
    from: IpAddr,
    ttl: Option<u8>,
    bytes: usize,
    time: Duration,
) -> Outcome {
    let message = match status {
        IP_SUCCESS => {
            return Outcome::Reply {
                from,
                ttl,
                bytes,
                time,
            };
        }
        IP_REQ_TIMED_OUT => return Outcome::TimedOut,
        IP_DEST_HOST_UNREACHABLE => "Destination Host Unreachable",
        IP_DEST_NET_UNREACHABLE => "Destination Net Unreachable",
        IP_DEST_PORT_UNREACHABLE => "Destination Port Unreachable",
        IP_TTL_EXPIRED_TRANSIT => "Time to live exceeded",
        _ => "Destination Unreachable",
    };
    Outcome::Error { from, message }
}

const fn sockaddr_in6(address: Ipv6Addr) -> SOCKADDR_IN6 {
    // SAFETY: SOCKADDR_IN6 is plain data, for which all-zero is valid.
    let mut raw: SOCKADDR_IN6 = unsafe { std::mem::zeroed() };
    raw.sin6_family = AF_INET6;
    raw.sin6_addr.u.Byte = address.octets();
    raw
}

/// The name `address` resolves back to, as `ping` shows it (`host (1.2.3.4)`); `None`
/// when it has none. Winsock must be initialised, as any earlier name lookup through
/// `std::net` does.
pub fn host_name(address: IpAddr) -> Option<String> {
    use windows_sys::Win32::Networking::WinSock::{
        AF_INET, GetNameInfoW, NI_MAXHOST, NI_NAMEREQD, SOCKADDR, SOCKADDR_IN,
    };

    let mut name = [0u16; NI_MAXHOST as usize];
    let result = match address {
        IpAddr::V4(v4) => {
            // SAFETY: SOCKADDR_IN is plain data, for which all-zero is valid.
            let mut raw: SOCKADDR_IN = unsafe { std::mem::zeroed() };
            raw.sin_family = AF_INET;
            raw.sin_addr.S_un.S_addr = u32::from_ne_bytes(v4.octets());
            // SAFETY: `raw` and `name` are valid for the call and sized as passed.
            unsafe {
                GetNameInfoW(
                    (&raw const raw).cast::<SOCKADDR>(),
                    i32::try_from(size_of::<SOCKADDR_IN>()).unwrap_or(0),
                    name.as_mut_ptr(),
                    NI_MAXHOST,
                    std::ptr::null_mut(),
                    0,
                    i32::try_from(NI_NAMEREQD).unwrap_or(0),
                )
            }
        }
        IpAddr::V6(v6) => {
            let raw = sockaddr_in6(v6);
            // SAFETY: as above.
            unsafe {
                GetNameInfoW(
                    (&raw const raw).cast::<SOCKADDR>(),
                    i32::try_from(size_of::<SOCKADDR_IN6>()).unwrap_or(0),
                    name.as_mut_ptr(),
                    NI_MAXHOST,
                    std::ptr::null_mut(),
                    0,
                    i32::try_from(NI_NAMEREQD).unwrap_or(0),
                )
            }
        }
    };
    if result != 0 {
        return None;
    }
    let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Some(String::from_utf16_lossy(&name[..len]))
}

/// Raised when the console delivers Ctrl-C or Ctrl-Break to this process, once
/// [`catch_interrupts`] has installed the handler.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Turns Ctrl-C and Ctrl-Break into a flag instead of ending the process, so a program
/// like `ping` can print its summary first. Returns the flag.
pub fn catch_interrupts() -> &'static AtomicBool {
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler,
    };

    unsafe extern "system" fn handler(kind: u32) -> i32 {
        if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
            INTERRUPTED.store(true, Ordering::SeqCst);
            1
        } else {
            0
        }
    }
    // SAFETY: registering a handler that only stores to an atomic.
    unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
    &INTERRUPTED
}
