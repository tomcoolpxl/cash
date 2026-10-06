//! What the shell and its network tools share.
//!
//! The service names a port goes by, the C library's words for a connection or a lookup
//! that failed, and the sockets behind `/dev/tcp/HOST/PORT` and `/dev/udp/HOST/PORT` in
//! a redirection, as Bash opens them.
//!
//! `exec 3<>/dev/tcp/host/80` connects a socket and puts it in the descriptor table, so
//! `echo ... >&3` and `read -r line <&3` talk to the server; `cat </dev/tcp/host/port`
//! and `echo ... >/dev/tcp/host/port` connect for one command. Only a redirection opens
//! these names, as in Bash: `cat /dev/tcp/host/port` looks for a file of that name and
//! finds none. The host is a name or an address, the port a number or a service name.

use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};

/// The service names a port is reported under, and that stand for a port on a command
/// line.
///
/// The common ones from `/etc/services`, since Windows' own table is not read. `nc`
/// reports and takes them, and `/dev/tcp/HOST/SERVICE` takes them.
pub const SERVICES: &[(&str, u16)] = &[
    ("ftp-data", 20),
    ("ftp", 21),
    ("ssh", 22),
    ("telnet", 23),
    ("smtp", 25),
    ("domain", 53),
    ("bootps", 67),
    ("bootpc", 68),
    ("tftp", 69),
    ("gopher", 70),
    ("finger", 79),
    ("http", 80),
    ("kerberos", 88),
    ("pop3", 110),
    ("sunrpc", 111),
    ("nntp", 119),
    ("ntp", 123),
    ("netbios-ns", 137),
    ("netbios-dgm", 138),
    ("netbios-ssn", 139),
    ("imap", 143),
    ("snmp", 161),
    ("snmp-trap", 162),
    ("ldap", 389),
    ("https", 443),
    ("microsoft-ds", 445),
    ("smtps", 465),
    ("syslog", 514),
    ("submission", 587),
    ("ldaps", 636),
    ("rsync", 873),
    ("imaps", 993),
    ("pop3s", 995),
    ("socks", 1080),
    ("ms-sql-s", 1433),
    ("nfs", 2049),
    ("mysql", 3306),
    ("ms-wbt-server", 3389),
    ("svn", 3690),
    ("sip", 5060),
    ("postgresql", 5432),
    ("rfb", 5900),
    ("x11", 6000),
    ("redis", 6379),
    ("irc", 6667),
    ("http-alt", 8080),
    ("memcache", 11211),
    ("mongodb", 27017),
];

/// The port a service name stands for.
#[must_use]
pub fn service_port(name: &str) -> Option<u16> {
    SERVICES
        .iter()
        .find(|(service, _)| *service == name)
        .map(|(_, port)| *port)
}

/// The name a port is reported under in `[tcp/NAME]`; `*` for one without.
#[must_use]
pub fn service_name(port: u16) -> &'static str {
    SERVICES
        .iter()
        .find(|(_, known)| *known == port)
        .map_or("*", |(name, _)| name)
}

/// A failed connection in `strerror`'s words: the ones scripts match on, as glibc has
/// them, from Windows' socket errors.
#[must_use]
pub fn connect_failure(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::ConnectionRefused => "Connection refused".to_owned(),
        ErrorKind::TimedOut => "Connection timed out".to_owned(),
        ErrorKind::ConnectionReset => "Connection reset by peer".to_owned(),
        ErrorKind::ConnectionAborted => "Software caused connection abort".to_owned(),
        ErrorKind::NetworkUnreachable => "Network is unreachable".to_owned(),
        ErrorKind::HostUnreachable => "No route to host".to_owned(),
        ErrorKind::AddrInUse => "Address already in use".to_owned(),
        ErrorKind::AddrNotAvailable => "Cannot assign requested address".to_owned(),
        ErrorKind::PermissionDenied => "Permission denied".to_owned(),
        _ => crate::error::os_error_text(error),
    }
}

/// A name lookup's failure in `gai_strerror`'s words: Windows' codes for the ones glibc
/// scripts know.
#[must_use]
pub fn lookup_failure(error: &std::io::Error) -> String {
    match error.raw_os_error() {
        Some(11001) => "Name or service not known".to_owned(),
        Some(11002) => "Temporary failure in name resolution".to_owned(),
        Some(11003) => "Non-recoverable failure in name resolution".to_owned(),
        Some(11004) => "No address associated with hostname".to_owned(),
        _ => crate::error::os_error_text(error),
    }
}

/// A socket a redirection opened: the descriptor behind `/dev/tcp/...` or `/dev/udp/...`.
#[derive(Debug)]
pub enum Socket {
    /// A TCP connection.
    Tcp(TcpStream),
    /// A UDP socket connected to one peer: writes are datagrams to it, reads datagrams
    /// from it.
    Udp(UdpSocket),
}

impl Socket {
    /// The socket's handle, for a program that is handed the descriptor.
    #[must_use]
    pub fn as_raw_socket(&self) -> std::os::windows::io::RawSocket {
        use std::os::windows::io::AsRawSocket as _;
        match self {
            Self::Tcp(stream) => stream.as_raw_socket(),
            Self::Udp(socket) => socket.as_raw_socket(),
        }
    }
}

impl std::io::Read for &Socket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Socket::Tcp(stream) => (&*stream).read(buf),
            Socket::Udp(socket) => socket.recv(buf),
        }
    }
}

impl std::io::Write for &Socket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Socket::Tcp(stream) => (&*stream).write(buf),
            Socket::Udp(socket) => socket.send(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Why `/dev/tcp/HOST/PORT` or `/dev/udp/HOST/PORT` did not open, with Bash's two lines
/// in mind: the first names what failed, the second (the redirection's own) says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketFailure {
    /// The host or the service was not known: Bash's `HOST: Name or service not known`
    /// or `PORT: Servname not supported for ai_socktype`, then the redirection fails
    /// with `Invalid argument`.
    Lookup {
        /// The host or service name that was not known.
        name: String,
        /// `gai_strerror`'s words.
        reason: String,
    },
    /// No address accepted the connection: Bash's `connect: Connection refused`, then
    /// the redirection fails with the same words.
    Connect(String),
}

impl SocketFailure {
    /// The words of the redirection's own error line.
    #[must_use]
    pub fn redirection_reason(&self) -> &str {
        match self {
            Self::Lookup { .. } => "Invalid argument",
            Self::Connect(reason) => reason,
        }
    }
}

/// The port `text` names: a number, or a service name from [`SERVICES`]. Bash hands an
/// empty port to `getaddrinfo`, which gives port 0.
fn port_number(text: &str) -> Option<u16> {
    if text.is_empty() {
        return Some(0);
    }
    text.parse().ok().or_else(|| service_port(text))
}

/// The addresses `host` names, in the order Bash tries them.
fn addresses(host: &str, port: u16) -> Result<Vec<SocketAddr>, SocketFailure> {
    if let Ok(address) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(address, port)]);
    }
    let found = (host, port)
        .to_socket_addrs()
        .map_err(|error| SocketFailure::Lookup {
            name: host.to_owned(),
            reason: lookup_failure(&error),
        })?;
    let addresses: Vec<SocketAddr> = found.collect();
    if addresses.is_empty() {
        return Err(SocketFailure::Lookup {
            name: host.to_owned(),
            reason: "Name or service not known".to_owned(),
        });
    }
    Ok(addresses)
}

/// Opens the socket a `/dev/tcp` or `/dev/udp` redirection names, as Bash's `netopen`
/// does: every address of the host is tried, and the last failure is the one reported.
///
/// # Errors
///
/// Returns [`SocketFailure`] when the host or port is not known, or no address accepted
/// the connection.
pub fn connect(name: &cash_win32::devices::SocketName) -> Result<Socket, SocketFailure> {
    use cash_win32::devices::SocketProtocol;

    let port = port_number(&name.port).ok_or_else(|| SocketFailure::Lookup {
        name: name.port.clone(),
        reason: "Servname not supported for ai_socktype".to_owned(),
    })?;
    let mut last = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    for address in addresses(&name.host, port)? {
        // Synchronous sockets (`cash_win32::sockets`), so that a program handed the
        // descriptor can read and write it as a standard stream.
        let attempt = match name.protocol {
            SocketProtocol::Tcp => cash_win32::sockets::connect_tcp(address).map(Socket::Tcp),
            SocketProtocol::Udp => cash_win32::sockets::connect_udp(address).map(Socket::Udp),
        };
        match attempt {
            Ok(socket) => return Ok(socket),
            Err(error) => last = error,
        }
    }
    Err(SocketFailure::Connect(connect_failure(&last)))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use super::*;

    #[test]
    fn the_service_table_is_sorted_by_port_and_has_no_repeats() {
        let ports: Vec<u16> = SERVICES.iter().map(|(_, port)| *port).collect();
        let mut sorted = ports.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ports, sorted);
        assert_eq!(service_port("https"), Some(443));
        assert_eq!(service_port("nothing"), None);
        assert_eq!(service_name(22), "ssh");
        assert_eq!(service_name(49152), "*");
    }

    #[test]
    fn a_port_is_a_number_a_service_or_empty() {
        assert_eq!(port_number("80"), Some(80));
        assert_eq!(port_number("http"), Some(80));
        assert_eq!(port_number(""), Some(0));
        assert_eq!(port_number("nosvc"), None);
        assert_eq!(port_number("70000"), None);
    }

    #[test]
    fn a_refused_port_is_a_connect_failure_in_glibcs_words() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let name = cash_win32::devices::SocketName {
            protocol: cash_win32::devices::SocketProtocol::Tcp,
            host: "127.0.0.1".to_owned(),
            port: port.to_string(),
        };
        let failure = connect(&name).unwrap_err();
        assert_eq!(
            failure,
            SocketFailure::Connect("Connection refused".to_owned())
        );
        assert_eq!(failure.redirection_reason(), "Connection refused");
    }

    #[test]
    fn an_unknown_service_is_a_lookup_failure() {
        let name = cash_win32::devices::SocketName {
            protocol: cash_win32::devices::SocketProtocol::Tcp,
            host: "127.0.0.1".to_owned(),
            port: "nosvc".to_owned(),
        };
        let failure = connect(&name).unwrap_err();
        assert_eq!(
            failure,
            SocketFailure::Lookup {
                name: "nosvc".to_owned(),
                reason: "Servname not supported for ai_socktype".to_owned(),
            }
        );
        assert_eq!(failure.redirection_reason(), "Invalid argument");
    }

    #[test]
    fn a_tcp_socket_round_trips_and_udp_sends_a_datagram() {
        use std::io::{Read as _, Write as _};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 16];
            let n = stream.read(&mut buffer).unwrap();
            stream.write_all(&buffer[..n]).unwrap();
        });
        let name = cash_win32::devices::SocketName {
            protocol: cash_win32::devices::SocketProtocol::Tcp,
            host: "localhost".to_owned(),
            port: port.to_string(),
        };
        let socket = connect(&name).unwrap();
        (&socket).write_all(b"ping\n").unwrap();
        let mut back = [0u8; 16];
        let n = (&socket).read(&mut back).unwrap();
        assert_eq!(&back[..n], b"ping\n");
        server.join().unwrap();

        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let name = cash_win32::devices::SocketName {
            protocol: cash_win32::devices::SocketProtocol::Udp,
            host: "127.0.0.1".to_owned(),
            port: receiver.local_addr().unwrap().port().to_string(),
        };
        let socket = connect(&name).unwrap();
        (&socket).write_all(b"datagram").unwrap();
        let mut buffer = [0u8; 16];
        let (n, _) = receiver.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], b"datagram");
    }
}
