//! `lsof` (ROADMAP item 8; the handle walk since TODO phase 17, 2026-10-04).
//!
//! Output follows lsof 4.99.7: the COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
//! columns, `-t` for bare process ids, and exit status 1 when nothing is listed.
//!
//! Where each answer comes from:
//!
//! * the processes holding given files, or files below a directory (`+D`, `+d`), from
//!   the Restart Manager;
//! * TCP and UDP sockets with their owners (`-i`), from IP Helper;
//! * for selected processes (`-p`, `-c`, `-u`), or every process with no selection: the
//!   executable (`txt`), loaded modules (`mem`), sockets, and the files and folders it
//!   holds open, from the handle walk in `cash_win32::handles` (FD is the handle value
//!   with `r`, `w` or `u`). That walk is what handle.exe does; unelevated it sees the
//!   processes the user may open, as Linux `lsof` without root, and a warning counts
//!   the others.
//!
//! Windows has no device or inode numbers, so DEVICE and NODE show `-` (NODE is
//! `TCP`/`UDP` for sockets, as in lsof), and FD is `-` where no handle is known.

use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

use cash_core::{ExecutionResult, builtins};
use cash_win32::handles::{self, OpenFile, Walk};
use cash_win32::{net, process};
use clap::Parser;

use crate::fileuse::{self, Access, ProcessNames, Services};

/// lsof's default COMMAND width.
const DEFAULT_COMMAND_WIDTH: usize = 9;

/// List open files and sockets.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct LsofCommand {
    /// Options and names, parsed here: lsof's grammar has `+` options and optional
    /// attached arguments (`-i:8080`, `-sTCP:LISTEN`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// One `-i` address specification.
#[derive(Default)]
struct InetSpec {
    v4: bool,
    v6: bool,
    proto: Option<net::Proto>,
    hosts: Option<Vec<IpAddr>>,
    ports: Vec<(u16, u16)>,
}

/// A list selection with `^` exclusions, as `-p` and `-u` take.
#[derive(Default)]
struct ListSelection<T> {
    include: Vec<T>,
    exclude: Vec<T>,
}

#[derive(Default)]
struct Options {
    terse: bool,
    numeric_hosts: bool,
    numeric_ports: bool,
    and: bool,
    quiet: bool,
    command_width: usize,
    inet: Option<Vec<InetSpec>>,
    tcp_states: Option<(Vec<String>, Vec<String>)>,
    pids: Option<ListSelection<u32>>,
    commands: Vec<String>,
    users: Option<ListSelection<String>>,
    files: Vec<String>,
    dirs: Vec<(String, bool)>,
}

/// One output row.
struct Row {
    pid: u32,
    fd: String,
    /// Where the row sorts within its process: the executable, modules, open handles by
    /// value, then sockets, as lsof orders cwd, txt, mem and the descriptors.
    rank: (u8, usize),
    kind: String,
    size: String,
    node: String,
    name: String,
    /// The file a row is about, for matching file selections.
    file: Option<String>,
    socket: Option<net::Socket>,
}

enum Parsed {
    Run(Box<Options>),
    Exit(ExecutionResult),
}

impl builtins::Command for LsofCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let services = Services::load();
        match parse(&self.args, &services, &context)? {
            Parsed::Run(options) => run(&options, &services, &context),
            Parsed::Exit(result) => Ok(result),
        }
    }
}

fn fail(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    message: &str,
) -> Result<Parsed, cash_core::Error> {
    writeln!(context.stderr(), "lsof: {message}")?;
    Ok(Parsed::Exit(ExecutionResult::general_error()))
}

fn looks_like_inet(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let rest = lower.trim_start_matches(['4', '6']);
    rest.is_empty()
        || rest.starts_with([':', '@'])
        || rest.starts_with("tcp")
        || rest.starts_with("udp")
}

fn parse_inet(text: &str, services: &Services) -> Option<InetSpec> {
    let mut spec = InetSpec::default();
    let mut rest = text;
    if let Some(stripped) = rest.strip_prefix('4') {
        spec.v4 = true;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix('6') {
        spec.v6 = true;
        rest = stripped;
    }
    for (name, proto) in [("tcp", net::Proto::Tcp), ("udp", net::Proto::Udp)] {
        if let Some(prefix) = rest.get(..3)
            && prefix.eq_ignore_ascii_case(name)
        {
            spec.proto = Some(proto);
            rest = rest.get(3..).unwrap_or("");
            break;
        }
    }
    // `@host`, `@[v6addr]`, then an optional `:ports` part in both forms.
    let ports = if let Some(after_at) = rest.strip_prefix('@') {
        let (host, after_host) = if let Some(bracketed) = after_at.strip_prefix('[') {
            bracketed.split_once(']')?
        } else {
            after_at
                .split_once(':')
                .map_or((after_at, ""), |(host, ports)| (host, ports))
        };
        spec.hosts = Some((host, 0).to_socket_addrs().ok()?.map(|a| a.ip()).collect());
        after_host.strip_prefix(':').unwrap_or(after_host)
    } else if let Some(ports) = rest.strip_prefix(':') {
        ports
    } else if rest.is_empty() {
        ""
    } else {
        return None;
    };

    let proto_name = spec.proto.map_or("tcp", net::Proto::name);
    for part in ports.split(',').filter(|p| !p.is_empty()) {
        let range = if let Some((low, high)) = part.split_once('-') {
            (
                services.port(low, proto_name)?,
                services.port(high, proto_name)?,
            )
        } else {
            let port = services.port(part, proto_name)?;
            (port, port)
        };
        spec.ports.push(range);
    }
    Some(spec)
}

fn split_list<T>(text: &str, item: impl Fn(&str) -> Option<T>) -> Option<ListSelection<T>> {
    let mut selection = ListSelection {
        include: Vec::new(),
        exclude: Vec::new(),
    };
    for part in text.split(',').filter(|p| !p.is_empty()) {
        match part.strip_prefix('^') {
            Some(excluded) => selection.exclude.push(item(excluded)?),
            None => selection.include.push(item(part)?),
        }
    }
    Some(selection)
}

#[expect(
    clippy::too_many_lines,
    reason = "lsof's option grammar is one flat list; splitting it would scatter it"
)]
fn parse(
    args: &[String],
    services: &Services,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Parsed, cash_core::Error> {
    let mut options = Options {
        command_width: DEFAULT_COMMAND_WIDTH,
        ..Options::default()
    };
    let mut args = args.iter().peekable();
    let mut only_names = false;

    while let Some(arg) = args.next() {
        if only_names || !(arg.starts_with('-') || arg.starts_with('+')) || arg.len() == 1 {
            options.files.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_names = true;
            continue;
        }
        match arg.as_str() {
            "-h" | "-?" | "--help" => {
                writeln!(
                    context.stdout(),
                    "usage: lsof [-anPtw] [+c w] [-c c] [-i i] [-p s] [-s p:s] [-u s] [+d s] [+D D] [--] [names]"
                )?;
                return Ok(Parsed::Exit(ExecutionResult::success()));
            }
            "-v" | "--version" => {
                writeln!(
                    context.stdout(),
                    "lsof (cash) {}: the Windows subset, see ROADMAP item 8",
                    env!("CARGO_PKG_VERSION")
                )?;
                return Ok(Parsed::Exit(ExecutionResult::success()));
            }
            "+D" | "+d" => {
                let Some(dir) = args.next() else {
                    return fail(context, &format!("{arg} requires a directory"));
                };
                options.dirs.push((dir.clone(), arg == "+D"));
                continue;
            }
            "+c" => {
                let Some(width) = args.next().and_then(|w| w.parse().ok()) else {
                    return fail(context, "+c requires a number");
                };
                options.command_width = if width == 0 { usize::MAX } else { width };
                continue;
            }
            _ => {}
        }
        if arg.starts_with('+') {
            return fail(context, &format!("unsupported option: {arg}"));
        }

        let body = arg.get(1..).unwrap_or("");
        for (index, flag) in body.char_indices() {
            let attached = body.get(index + flag.len_utf8()..).unwrap_or("");
            match flag {
                't' => options.terse = true,
                'n' => options.numeric_hosts = true,
                'P' => options.numeric_ports = true,
                'a' => options.and = true,
                'w' => options.quiet = true,
                // Numeric user ids and non-blocking kernel calls mean nothing here.
                'l' | 'b' => {}
                'i' => {
                    let text = if attached.is_empty() {
                        match args.peek() {
                            Some(next) if looks_like_inet(next) => args.next().cloned(),
                            _ => None,
                        }
                    } else {
                        Some(attached.to_owned())
                    };
                    let spec = match text {
                        Some(text) => match parse_inet(&text, services) {
                            Some(spec) => spec,
                            None => {
                                return fail(
                                    context,
                                    &format!("unacceptable Internet address: {text}"),
                                );
                            }
                        },
                        None => InetSpec::default(),
                    };
                    options.inet.get_or_insert_with(Vec::new).push(spec);
                    break;
                }
                's' => {
                    if attached.is_empty() {
                        // Plain `-s` asks for the size column, which is always shown.
                        continue;
                    }
                    let Some((proto, states)) = attached.split_once(':') else {
                        return fail(
                            context,
                            &format!("unsupported -s specification: {attached}"),
                        );
                    };
                    if !proto.eq_ignore_ascii_case("tcp") {
                        return fail(context, &format!("-s {proto}: only TCP states exist"));
                    }
                    let (mut include, mut exclude) = (Vec::new(), Vec::new());
                    for state in states.split(',') {
                        match state.strip_prefix('^') {
                            Some(s) => exclude.push(s.to_ascii_uppercase()),
                            None => include.push(state.to_ascii_uppercase()),
                        }
                    }
                    options.tcp_states = Some((include, exclude));
                    break;
                }
                'p' | 'c' | 'u' => {
                    let value = if attached.is_empty() {
                        match args.next() {
                            Some(value) => value.clone(),
                            None => return fail(context, &format!("-{flag} requires an argument")),
                        }
                    } else {
                        attached.to_owned()
                    };
                    match flag {
                        'p' => match split_list(&value, |p| p.parse().ok()) {
                            Some(selection) => options.pids = Some(selection),
                            None => {
                                return fail(context, &format!("illegal process ID: {value}"));
                            }
                        },
                        'c' => options.commands.push(value),
                        _ => {
                            options.users = split_list(&value, |u| Some(u.to_ascii_lowercase()));
                        }
                    }
                    break;
                }
                'U' => {
                    return fail(
                        context,
                        "-U: Unix domain sockets cannot be listed on Windows",
                    );
                }
                'd' | 'F' | 'r' | 'R' | 'D' | 'N' | 'K' | 'f' | 'g' | 'o' | 'S' | 'T' | 'x' => {
                    return fail(
                        context,
                        &format!("-{flag}: not supported by cash's lsof (see `lsof -h`)"),
                    );
                }
                other => {
                    return fail(context, &format!("illegal option character: {other}"));
                }
            }
        }
    }

    Ok(Parsed::Run(Box::new(options)))
}

fn host_text(ip: IpAddr, numeric: bool) -> String {
    if ip.is_unspecified() {
        return "*".to_owned();
    }
    if !numeric && ip.is_loopback() {
        return "localhost".to_owned();
    }
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

fn endpoint(addr: SocketAddr, proto: net::Proto, options: &Options, services: &Services) -> String {
    let port = if options.numeric_ports {
        None
    } else {
        services.name(addr.port(), proto.name())
    };
    let port = port.map_or_else(|| addr.port().to_string(), str::to_owned);
    format!("{}:{port}", host_text(addr.ip(), options.numeric_hosts))
}

fn socket_row(socket: net::Socket, options: &Options, services: &Services) -> Row {
    let mut name = endpoint(socket.local, socket.proto, options, services);
    if let Some(remote) = socket.remote {
        name.push_str("->");
        name.push_str(&endpoint(remote, socket.proto, options, services));
    }
    if let Some(state) = socket.state {
        name.push_str(" (");
        name.push_str(state.lsof_name());
        name.push(')');
    }
    Row {
        pid: socket.pid,
        fd: "-".to_owned(),
        rank: (3, 0),
        kind: if socket.local.is_ipv6() {
            "IPv6"
        } else {
            "IPv4"
        }
        .to_owned(),
        size: "0t0".to_owned(),
        node: match socket.proto {
            net::Proto::Tcp => "TCP",
            net::Proto::Udp => "UDP",
        }
        .to_owned(),
        name,
        file: None,
        socket: Some(socket),
    }
}

fn file_row(pid: u32, path: &Path, display: String, access: Access) -> Row {
    let metadata = std::fs::metadata(path).ok();
    let (fd, rank) = match access {
        Access::Executable => ("txt", 0),
        Access::Mapped => ("mem", 1),
        Access::Open => ("-", 2),
    };
    Row {
        pid,
        fd: fd.to_owned(),
        rank: (rank, 0),
        kind: if metadata.as_ref().is_some_and(std::fs::Metadata::is_dir) {
            "DIR"
        } else {
            "REG"
        }
        .to_owned(),
        size: metadata.map_or_else(|| "-".to_owned(), |m| m.len().to_string()),
        node: "-".to_owned(),
        name: display,
        file: Some(fileuse::path_key(path)),
        socket: None,
    }
}

/// Says what the walk could not look at: processes it could not open, and handles that
/// did not answer.
fn warn_about(walk: &Walk, stderr: &mut impl Write) -> std::io::Result<()> {
    if !walk.unopened.is_empty() {
        let advice = if process::current_process_is_elevated().unwrap_or(false) {
            "they are protected"
        } else {
            "they are another account's or elevated; `sudo lsof` lists them"
        };
        writeln!(
            stderr,
            "lsof: WARNING: can't list the files of {} processes: {advice}",
            walk.unopened.len()
        )?;
    }
    if walk.abandoned > 0 {
        writeln!(
            stderr,
            "lsof: WARNING: {} handles did not answer and are not listed",
            walk.abandoned
        )?;
    }
    Ok(())
}

/// Folders below this many files have the processes the walk could not open looked for
/// with the Restart Manager; above it, which opens every file, a warning says so instead.
const RESTART_MANAGER_FOLDER_LIMIT: usize = 20_000;

/// What processes hold at or below the folder `root` (directly in it unless `recursive`),
/// named in the user's spelling `shown`: open files from the handle walk, executables and
/// modules from every process, and for the processes the walk could not open, the
/// Restart Manager over the files below.
fn rows_below(
    root: &Path,
    shown: &str,
    recursive: bool,
    walk: Option<&Walk>,
    options: &Options,
    stderr: &mut impl Write,
) -> std::io::Result<Vec<Row>> {
    let root_key = fileuse::path_key(root);
    let root_key = root_key.trim_end_matches('\\');
    let inside = |path: &Path| {
        let key = fileuse::final_key(path);
        key.strip_prefix(root_key).is_some_and(|rest| {
            rest.is_empty()
                || rest
                    .strip_prefix('\\')
                    .is_some_and(|rest| recursive || !rest.contains('\\'))
        })
    };
    // Named as the user named the folder: `./held.txt` for `+D .`.
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical = canonical.to_string_lossy();
    let depth = Path::new(canonical.strip_prefix(r"\\?\").unwrap_or(&canonical))
        .components()
        .count();
    let display = |path: &Path| {
        let relative: PathBuf = path.components().skip(depth).collect();
        if relative.as_os_str().is_empty() {
            shown.to_owned()
        } else {
            format!(
                "{}/{}",
                shown.trim_end_matches(['/', '\\']),
                cash_win32::path::render(&relative)
            )
        }
    };

    let mut rows = Vec::new();
    for file in walk.iter().flat_map(|w| &w.files) {
        if inside(&file.path) {
            let mut row = handle_row(file);
            row.name = display(&file.path);
            rows.push(row);
        }
    }
    for listed in process::list() {
        let image = process::image_path(listed.pid);
        if let Some(image) = image.as_ref().filter(|image| inside(image)) {
            rows.push(file_row(
                listed.pid,
                image,
                display(image),
                Access::Executable,
            ));
        }
        for module in process::modules(listed.pid).unwrap_or_default() {
            if inside(&module) && image.as_ref() != Some(&module) {
                rows.push(file_row(
                    listed.pid,
                    &module,
                    display(&module),
                    Access::Mapped,
                ));
            }
        }
    }

    // The processes the walk could not open; all of them when there was no walk.
    let wanted: BTreeSet<u32> = walk.map_or_else(
        || process::list().into_iter().map(|p| p.pid).collect(),
        |w| w.unopened.clone(),
    );
    if wanted.is_empty() {
        return Ok(rows);
    }
    let files: Option<Vec<PathBuf>> = if recursive {
        fileuse::files_below_within(root, RESTART_MANAGER_FOLDER_LIMIT)
    } else {
        std::fs::read_dir(root).map_or(Some(Vec::new()), |entries| {
            let entries: Vec<_> = entries
                .flatten()
                .take(RESTART_MANAGER_FOLDER_LIMIT + 1)
                .collect();
            (entries.len() <= RESTART_MANAGER_FOLDER_LIMIT).then(|| {
                entries
                    .iter()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                    .map(std::fs::DirEntry::path)
                    .collect()
            })
        })
    };
    let Some(files) = files else {
        if !options.quiet {
            writeln!(
                stderr,
                "lsof: WARNING: {shown} holds over {RESTART_MANAGER_FOLDER_LIMIT} entries; the {} processes lsof can't open are not looked for in it",
                wanted.len()
            )?;
        }
        return Ok(rows);
    };
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    for (pid, file) in fileuse::wanted_holders(&refs, &wanted).unwrap_or_default() {
        rows.push(file_row(pid, &file, display(&file), Access::Open));
    }
    Ok(rows)
}

/// A file the handle walk found open: FD is the handle's value with lsof's mode letter,
/// `r`, `w` or `u` for both.
fn handle_row(file: &OpenFile) -> Row {
    let mode = match (file.read, file.write) {
        (true, true) => "u",
        (false, true) => "w",
        (true, false) => "r",
        (false, false) => "",
    };
    Row {
        pid: file.pid,
        fd: format!("{}{mode}", file.handle),
        rank: (2, file.handle),
        kind: if file.directory { "DIR" } else { "REG" }.to_owned(),
        size: file.size.map_or_else(|| "-".to_owned(), |s| s.to_string()),
        node: "-".to_owned(),
        name: cash_win32::path::render(&file.path),
        file: Some(fileuse::final_key(&file.path)),
        socket: None,
    }
}

fn inet_matches(spec: &InetSpec, socket: &net::Socket) -> bool {
    (spec.v4 == spec.v6
        || (spec.v4 && socket.local.is_ipv4())
        || (spec.v6 && socket.local.is_ipv6()))
        && spec.proto.is_none_or(|p| p == socket.proto)
        && spec.hosts.as_ref().is_none_or(|hosts| {
            hosts.contains(&socket.local.ip())
                || socket.remote.is_some_and(|r| hosts.contains(&r.ip()))
        })
        && (spec.ports.is_empty()
            || spec.ports.iter().any(|&(low, high)| {
                (low..=high).contains(&socket.local.port())
                    || socket
                        .remote
                        .is_some_and(|r| (low..=high).contains(&r.port()))
            }))
}

fn state_matches(states: &(Vec<String>, Vec<String>), socket: &net::Socket) -> bool {
    let Some(state) = socket.state else {
        return false;
    };
    let name = state.lsof_name();
    (states.0.is_empty() || states.0.iter().any(|s| s == name))
        && !states.1.iter().any(|s| s == name)
}

#[expect(
    clippy::too_many_lines,
    reason = "gathering, selecting and printing rows is one pipeline"
)]
fn run(
    options: &Options,
    services: &Services,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<ExecutionResult, cash_core::Error> {
    let mut processes = ProcessNames::new();
    let mut stderr = context.stderr();
    let mut rows: Vec<Row> = Vec::new();
    let mut file_keys: HashSet<String> = HashSet::new();
    // With no selection at all, every process is selected, as in lsof.
    let everything = options.files.is_empty()
        && options.dirs.is_empty()
        && options.inet.is_none()
        && options.pids.is_none()
        && options.commands.is_empty()
        && options.users.is_none();

    // Files, from the Restart Manager; folders, `+D` (all below), `+d` (directly in) and a
    // folder named as a file (all below), from the handle walk.
    let mut folders: Vec<(PathBuf, &str, bool)> = Vec::new();
    for name in &options.files {
        let path = context.shell.absolute_path(name);
        if !path.exists() {
            if !options.quiet {
                writeln!(
                    stderr,
                    "lsof: status error on {name}: No such file or directory"
                )?;
            }
            continue;
        }
        if path.is_dir() {
            folders.push((path, name, true));
            continue;
        }
        file_keys.insert(fileuse::path_key(&path));
        match fileuse::file_holders(&path) {
            Ok(holders) => rows.extend(
                holders
                    .into_iter()
                    .map(|h| file_row(h.pid, &h.path, name.clone(), h.access)),
            ),
            Err(error) => writeln!(
                stderr,
                "lsof: {name}: {}",
                cash_core::error::os_error_text(&error)
            )?,
        }
    }
    for (dir, recursive) in &options.dirs {
        let path = context.shell.absolute_path(dir);
        if path.is_dir() {
            folders.push((path, dir, *recursive));
        } else {
            writeln!(
                stderr,
                "lsof: WARNING: can't stat({dir}): No such directory"
            )?;
        }
    }
    // Every process's handles, for folders and for `lsof` alone; otherwise only the
    // selected processes' are walked, below.
    let full_walk = (everything || !folders.is_empty()).then(|| handles::open_files(None));
    if let Some(Err(error)) = &full_walk {
        let error = cash_core::error::os_error_text(error);
        writeln!(stderr, "lsof: can't list open files: {error}")?;
    }
    let full_walk = full_walk.and_then(Result::ok);
    for (root, shown, recursive) in &folders {
        let found = rows_below(
            root,
            shown,
            *recursive,
            full_walk.as_ref(),
            options,
            &mut stderr,
        )?;
        file_keys.extend(found.iter().filter_map(|row| row.file.clone()));
        rows.extend(found);
    }

    // Sockets, needed for -i and for the processes -p/-c/-u select.
    let selects_processes = everything
        || options.pids.is_some()
        || !options.commands.is_empty()
        || options.users.is_some();
    let sockets = if options.inet.is_some() || selects_processes {
        let (v4, v6) = options.inet.as_ref().map_or((true, true), |specs| {
            let any_v4 = specs.iter().any(|s| s.v4);
            let any_v6 = specs.iter().any(|s| s.v6);
            if any_v4 || any_v6 {
                (any_v4, any_v6)
            } else {
                (true, true)
            }
        });
        match net::sockets(&[net::Proto::Tcp, net::Proto::Udp], v4, v6) {
            Ok(sockets) => sockets.into_iter().filter(|s| s.pid != 0).collect(),
            Err(error) => {
                let error = cash_core::error::os_error_text(&error);
                writeln!(stderr, "lsof: can't read the socket tables: {error}")?;
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    rows.extend(
        sockets
            .into_iter()
            .map(|s| socket_row(s, options, services)),
    );

    // The executable and modules of processes selected by -p, -c or -u.
    let pid_selected = |pid: u32, processes: &mut ProcessNames| -> (bool, bool, bool) {
        let by_pid = options.pids.as_ref().is_some_and(|sel| {
            (sel.include.is_empty() || sel.include.contains(&pid)) && !sel.exclude.contains(&pid)
        });
        let by_command = options.commands.iter().any(|c| {
            let name = processes.name(pid).to_ascii_lowercase();
            name.starts_with(&c.to_ascii_lowercase())
        });
        let by_user = options.users.as_ref().is_some_and(|sel| {
            let user = processes.user(pid).to_ascii_lowercase();
            (sel.include.is_empty() || sel.include.contains(&user)) && !sel.exclude.contains(&user)
        });
        (by_pid, by_command, by_user)
    };
    if selects_processes && !(options.and && (options.inet.is_some() || !file_keys.is_empty())) {
        let candidates: BTreeSet<u32> = process::list()
            .into_iter()
            .map(|p| p.pid)
            .filter(|&pid| {
                let (p, c, u) = pid_selected(pid, &mut processes);
                everything || p || c || u
            })
            .collect();
        // Their open files, from the handle walk: the one already made for `lsof` alone
        // or for a folder (which also has the processes started since `process::list`),
        // else one of these processes only.
        let walked = if full_walk.is_some() {
            Ok(None)
        } else {
            handles::open_files(Some(&candidates)).map(Some)
        };
        match walked {
            Ok(own) => {
                if let Some(walk) = own.as_ref().or(full_walk.as_ref()) {
                    rows.extend(
                        walk.files
                            .iter()
                            .filter(|f| everything || candidates.contains(&f.pid))
                            .map(handle_row),
                    );
                    if !options.terse && !options.quiet {
                        warn_about(walk, &mut stderr)?;
                    }
                }
            }
            Err(error) => {
                let error = cash_core::error::os_error_text(&error);
                writeln!(stderr, "lsof: can't list open files: {error}")?;
            }
        }
        for pid in candidates {
            if let Some(image) = process::image_path(pid) {
                rows.push(file_row(
                    pid,
                    &image,
                    cash_win32::path::render(&image),
                    Access::Executable,
                ));
            }
            if let Some(modules) = process::modules(pid) {
                let image_key = process::image_path(pid).map(|i| fileuse::path_key(&i));
                for module in modules {
                    if image_key.as_deref() == Some(fileuse::path_key(&module).as_str()) {
                        continue;
                    }
                    rows.push(file_row(
                        pid,
                        &module,
                        cash_win32::path::render(&module),
                        Access::Mapped,
                    ));
                }
            }
        }
    }

    // Selection: any given selection matches (OR), or all of them with -a.
    let selected: Vec<Row> = rows
        .into_iter()
        .filter(|row| {
            if everything {
                return true;
            }
            let mut tests: Vec<bool> = Vec::new();
            if !options.files.is_empty() || !options.dirs.is_empty() {
                tests.push(row.file.as_ref().is_some_and(|k| file_keys.contains(k)));
            }
            if let Some(specs) = &options.inet {
                tests.push(row.socket.as_ref().is_some_and(|s| {
                    specs.iter().any(|spec| inet_matches(spec, s))
                        && options
                            .tcp_states
                            .as_ref()
                            .is_none_or(|st| state_matches(st, s))
                }));
            }
            let (p, c, u) = pid_selected(row.pid, &mut processes);
            if options.pids.is_some() {
                tests.push(p);
            }
            if !options.commands.is_empty() {
                tests.push(c);
            }
            if options.users.is_some() {
                tests.push(u);
            }
            if options.and {
                tests.iter().all(|&t| t)
            } else {
                tests.iter().any(|&t| t)
            }
        })
        .collect();

    if selected.is_empty() {
        return Ok(ExecutionResult::general_error());
    }
    // Per process, the executable first, then modules, then everything else, as lsof
    // orders them.
    let mut selected = selected;
    selected.sort_by_key(|row| (row.pid, row.rank));

    let mut stdout = context.stdout();
    if options.terse {
        let pids: BTreeSet<u32> = selected.iter().map(|r| r.pid).collect();
        for pid in pids {
            writeln!(stdout, "{pid}")?;
        }
        return Ok(ExecutionResult::success());
    }

    let table: Vec<[String; 9]> = selected
        .into_iter()
        .map(|row| {
            // The 9-column COMMAND field is lsof's; dropping `.exe` first keeps
            // `python.exe` readable as `python` rather than `python.ex`.
            let name = processes.name(row.pid);
            let stem = name
                .strip_suffix(".exe")
                .or_else(|| name.strip_suffix(".EXE"))
                .unwrap_or(name);
            let command: String = stem.chars().take(options.command_width).collect();
            [
                command,
                row.pid.to_string(),
                processes.user(row.pid),
                row.fd,
                row.kind,
                "-".to_owned(),
                row.size,
                row.node,
                row.name,
            ]
        })
        .collect();
    let headers = [
        "COMMAND", "PID", "USER", "FD", "TYPE", "DEVICE", "SIZE/OFF", "NODE", "NAME",
    ];
    let widths: Vec<usize> = (0..9)
        .map(|i| {
            table
                .iter()
                .map(|r| r[i].chars().count())
                .chain(std::iter::once(headers[i].len()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    // COMMAND, USER, FD and NAME are left-aligned; the rest right-aligned, as in lsof.
    let left = [true, false, true, true, false, false, false, false, true];
    let line = |cells: &[&str]| -> String {
        let mut out = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            if i == 8 {
                out.push_str(cell);
            } else {
                let pad = " ".repeat(widths[i].saturating_sub(cell.chars().count()));
                if left[i] {
                    out.push_str(cell);
                    out.push_str(&pad);
                } else {
                    out.push_str(&pad);
                    out.push_str(cell);
                }
            }
        }
        out
    };
    writeln!(stdout, "{}", line(&headers))?;
    for row in &table {
        let cells: Vec<&str> = row.iter().map(String::as_str).collect();
        writeln!(stdout, "{}", line(&cells))?;
    }
    Ok(ExecutionResult::success())
}
