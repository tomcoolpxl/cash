# `ss` foundation evaluation

## Status

**Decided 2026-09-25.** The evaluation below was written as a discussion document; the
decisions taken on it are:

| Question | Decision |
|---|---|
| Build `ss` at all, and when | Yes, after `fuser`/`lsof`, reusing their socket layer (ROADMAP item 8) |
| Implementation | Directly on `windows-sys` in `cash-win32`; `netstat2` and Portview's `windows.rs` are references only |
| `Recv-Q` / `Send-Q` | Printed as `0`, keeping Linux column positions; documented in the spec as a divergence |
| `-p` on svchost | Adds a `service=NAME` field inside `users:((...))` |
| netstat flags (`ss -ano`) | Refused as usual (status 1) plus a one-line hint giving the `ss` spelling (`ss -tuanp`) |
| `-i` | Refused in the first version |

The rest of the document records what the four suggested projects could contribute and
what the shared socket-table layer for `ss`, `fuser`, and an `lsof` subset looks like.

## Recommendation (proposed)

Write the socket-table layer directly on `windows-sys` in `cash-win32`, beside the
existing `process::list` and `sysinfo` code. Do not depend on any of the four projects.
Use `netstat2` and Portview's `src/windows.rs` as references for buffer sizing,
byte-order handling, and IPv6 row layout.

Why:

- Cash already pins `windows-sys` 0.61 with `Win32_NetworkManagement_IpHelper`,
  `Win32_Networking_WinSock`, and `Win32_System_Diagnostics_ToolHelp` enabled.
  `GetExtendedTcpTable`, `GetExtendedUdpTable`, the `MIB_TCP(6)ROW_OWNER_PID` and
  `_OWNER_MODULE` structures, `GetOwnerModuleFromTcpEntry`, `GetTcpStatisticsEx`, and
  `GetPerTcpConnectionEStats` are all in the enabled features. No new crate and no new
  feature flag is needed for the core layer. `fuser`/`lsof` would add only
  `Win32_System_RestartManager`.
- Process names come from `cash_win32::process::list()`
  (`crates/cash-win32/src/process.rs`), which already walks a Toolhelp snapshot. Joining
  it with the socket tables by PID gives `ss -p`.
- The work is about 300–450 lines: four table calls (TCP/UDP × v4/v6) with the
  grow-and-retry buffer loop, row decoding (network-order ports, `dwState` → state
  name, IPv6 scope IDs), and a small `Socket` struct. `ss` argument parsing, filters, and
  output are separate and would be the same size under every option.
- `netstat2` is the only candidate usable as a library. Its Windows backend uses its own
  hand-written FFI instead of `windows-sys`, returns only PIDs (no names, modules, or
  statistics), and exposes no `_OWNER_MODULE` or EStats data. `ss -p`, `-s`, `-i`, and
  svchost service names would still need Cash's own Win32 code, so the dependency would
  cover only the easiest part.
- The other three are applications with TUI and other heavy dependencies. Portview and
  psnet are MIT and could be copied from; RustNet is Apache-2.0, so any copied code would
  need its notice carried in `NOTICE`. None of them offers a library API.

If writing the layer directly is rejected, the fallback is `netstat2 = "=0.11.2"`
(MIT OR Apache-2.0, Windows dependencies only `bitflags` and `thiserror`) behind a
Cash-owned wrapper type, so it can later be replaced without touching `ss`, `fuser`, or
`lsof`.

## Per-project assessment

Checked on 2026-09-25 from the GitHub repositories, crates.io, and docs.rs.

### netstat2 (`ohadravid/netstat2-rs`)

- **Licence:** MIT OR Apache-2.0. Compatible.
- **Maintenance:** 0.11.2, released 2025-08-14; the last commit on master is that
  release (bindgen and CI bump). 59 stars, 2 open issues, 1 open PR. Slow-moving but not
  abandoned. It is a maintained fork of ivxvm's original `netstat` crate.
- **Adoption:** about 1.0M total downloads and about 242k recent ones. Its 52 reverse
  dependencies include `bandwhich`, `somo`, `nu_plugin_port_list`, `rkill`, and
  `ports-cli`. Among Rust tools that need a portable socket-to-PID table, this is the
  common base.
- **Windows:** `GetExtendedTcpTable`/`GetExtendedUdpTable` from iphlpapi, IPv4 and IPv6,
  owner PID. FFI is hand-written in `src/integrations/windows/ffi/` (its manifest has no
  Windows target dependencies). No `_OWNER_MODULE` tables, no EStats, no process names.
  No admin needed.
- **Dependencies:** on Windows only `bitflags` 2 and `thiserror` 2. `bindgen` is a build
  dependency only on Linux, Android, macOS, and iOS. No async runtime.
- **API:** a library. `get_sockets_info(AddressFamilyFlags, ProtocolFlags)` returns
  `Vec<SocketInfo>`, and there are iterator variants with and without PIDs.
  `TcpSocketInfo` has local and remote address and port plus a 13-variant `TcpState`;
  `UdpSocketInfo` has only the local end. `associated_pids: Vec<u32>`. `uid` and `inode`
  exist only on Linux and Android.
- **`ss` coverage:** `-t -u -l -a -n -4 -6` and state filters directly; `-p` needs a
  separate PID-to-name join; nothing for `-s`, `-i`, `-e`, or `-m`.

### Portview (`Mapika/portview`)

- **Licence:** MIT. Compatible.
- **Maintenance:** active. v2.1.0, with commits on 2026-09-21 (Dependabot merges).
  59 stars, 1 open issue.
- **Windows:** `src/windows.rs` is about 650 lines on `windows-sys` 0.61 (the version
  Cash uses). It calls `GetExtendedTcpTable(TCP_TABLE_OWNER_PID_ALL)` and
  `GetExtendedUdpTable(UDP_TABLE_OWNER_PID)` for `AF_INET` and `AF_INET6`, then
  `QueryFullProcessImageNameW`, `K32GetProcessMemoryInfo`, `GetProcessTimes`, and
  `OpenProcessToken` + `LookupAccountSidW` for the process owner, plus a Toolhelp
  snapshot for child processes. It shows `-` when a process cannot be opened instead of
  dropping the row. No admin needed; no pcap.
- **Dependencies:** `clap`, `clap_complete`, `ratatui` 0.30, `crossterm` 0.29; `libc` on
  Unix. It also has Docker, SSH, and MCP-server modes.
- **API:** an application only, with no library target.
- **Value to Cash:** the closest reference implementation, since it uses the same
  `windows-sys` version and the same calls Cash would make. Code can be adapted with an
  MIT attribution. Its "listening plus stale `TIME_WAIT`/`CLOSE_WAIT`" views are good
  ideas for `ss -s`.

### RustNet (`domcyrus/rustnet`)

- **Licence:** Apache-2.0. Compatible with MIT for use or copying, but copied portions
  must keep the Apache notice and `NOTICE` requirements.
- **Maintenance:** very active. v1.6.0, with commits on 2026-09-25. About 5.1k stars,
  19 open issues.
- **Windows:** a packet-capture monitor. It requires **Npcap** (WinPcap-compatible mode)
  for capture. Process attribution uses an ETW kernel network/process trace, which may
  need Administrator or Performance Log Users membership, and falls back to
  `GetExtendedTcpTable`/`GetExtendedUdpTable` snapshots.
- **Dependencies:** heavy. `pcap`, `ratatui` (all widgets), `crossterm`, `crossbeam`,
  `dashmap`, `chrono`, `arboard`, `toml`, the `windows` 0.62 crate (not `windows-sys`),
  four internal workspace crates, and a Windows build script that downloads with
  `http_req`/`zip`/`sha2`.
- **API:** an application. The workspace crates (`rustnet-core`, `rustnet-host`) are not
  designed as a public library.
- **Value to Cash:** conceptual only. The ETW approach shows how to catch short-lived
  connections that snapshots miss, which `ss` does not need. Out of scope for a builtin.

### psnet (`psmux/psnet`)

- **Licence:** MIT. Compatible.
- **Maintenance:** active. v1.1.0, with commits on 2026-08-10 (including "PID column and
  svchost service name resolution"). 149 stars, 4 open issues.
- **Windows:** Windows-only. It uses iphlpapi TCP/UDP tables, `dnsapi` for the DNS
  cache, `ws2_32` raw sockets for packet dissection (raw sockets need admin), and the
  Windows Firewall COM interface. No Npcap.
- **Dependencies:** `ratatui` 0.29, `crossterm` 0.28, `sysinfo` 0.32, `maxminddb`,
  `dns-lookup`, `chrono`, `serde_json`, `dirs`, and `windows-sys` 0.59
  (`IpHelper`/`Ndis`/`WinSock`). It embeds about 7 MB of GeoIP data plus fingerprint and
  OUI databases, for a 12 MB executable.
- **API:** an application.
- **Value to Cash:** a reference for **svchost service-name resolution**, which Cash could
  offer as `ss -p` output like `users:(("svchost.exe",pid=1234,service="Dnscache"))`.
  The same result can come from `TCP_TABLE_OWNER_MODULE_ALL` +
  `GetOwnerModuleFromTcpEntry`, or from the service control manager's PID-to-service map.

## Comparison

| | netstat2 | Portview | RustNet | psnet | Direct `windows-sys` |
|---|---|---|---|---|---|
| Licence | MIT/Apache-2.0 | MIT | Apache-2.0 | MIT | Cash (MIT) |
| Last activity | 2025-08 | 2026-09 | 2026-09 | 2026-08 | n/a |
| Form | Library | App | App | App | Cash module |
| Usable as a crate | Yes | No | No | No | n/a |
| Windows bindings | Own FFI | windows-sys 0.61 | windows 0.62 | windows-sys 0.59 | windows-sys 0.61 (already present) |
| New dependencies for Cash | netstat2 only (bitflags 2, thiserror 2 already in `Cargo.lock`) | TUI stack | pcap, TUI, and more | TUI, GeoIP | None |
| Needs admin / Npcap | No / No | No / No | ETW may need admin / Npcap required | Raw sockets need admin / No | No for the core; admin only for EStats and kill |
| TCP/UDP × IPv4/IPv6 | Yes | Yes | Yes | Yes | Yes |
| Owner PID | Yes | Yes | Yes | Yes | Yes |
| Process name | No | Yes | Yes | Yes | Yes (via `process::list`) |
| Owner module or service | No | No | No | Yes (svchost) | Yes (`_OWNER_MODULE`) |
| Protocol statistics (`-s`) | No | Partial | Own counters | Own counters | `GetTcpStatisticsEx`/`GetUdpStatisticsEx` |
| Per-connection info (`-i`) | No | No | From capture | From capture | `GetPerTcpConnectionEStats` |

## Shared layer sketch

A single `cash_win32::net` module that `ss`, `fuser -n tcp|udp`, and `lsof -i` would all
call:

```rust
pub enum Proto { Tcp, Udp }
pub struct Socket {
    pub proto: Proto,
    pub local: SocketAddr,          // IPv6 carries scope_id
    pub remote: Option<SocketAddr>, // None for UDP
    pub state: Option<TcpState>,    // None for UDP
    pub pid: u32,
}
pub fn sockets(protos: &[Proto], v4: bool, v6: bool) -> io::Result<Vec<Socket>>;
pub fn owner_module(sock: &Socket) -> Option<String>; // service name for svchost
pub fn tcp_stats(v6: bool) -> io::Result<TcpStats>;
```

Rules the layer should follow:

- **Buffer loop:** call with a null buffer to get the size, allocate, and retry on
  `ERROR_INSUFFICIENT_BUFFER` a bounded number of times, because the table can grow
  between calls.
- **Byte order:** ports are in network byte order in the low 16 bits of a `u32`. IPv4
  addresses are network order; IPv6 addresses are 16 raw bytes plus a scope ID.
- **State names:** map `MIB_TCP_STATE_*` to Linux names: `LISTEN`, `ESTAB`, `SYN-SENT`,
  `SYN-RECV`, `FIN-WAIT-1`, `FIN-WAIT-2`, `TIME-WAIT`, `CLOSE-WAIT`, `LAST-ACK`,
  `CLOSING`, `CLOSED`. `DELETE_TCB` appears as `CLOSED`.
- **PID 0 and 4:** Windows reports `TIME_WAIT` rows with PID 0 and kernel-owned sockets
  with PID 4 (System). Show no `users:` field for PID 0, matching Linux's behaviour for
  sockets no process owns.
- **Least privilege:** never require elevation for the default path. When something is
  missing only because of access rights (a process name or module that cannot be read),
  omit that field and keep the row, as Portview does.

## Proposed `ss` feature matrix

A "refused" option must fail with a clear `ss: -X: not supported on Windows` message and
exit status 1 rather than being silently ignored. This follows the convention of Cash's
other bundled tools.

| Option | Proposal | Backing |
|---|---|---|
| `-t`, `-u` | Supported | TCP/UDP tables |
| `-l`, `-a` | Supported | Filter on `LISTEN`; UDP counts as listening, as on Linux |
| `-n` | Supported | Numeric output is the default path |
| `-r` | Supported (later) | `getnameinfo`; slow, so off by default like Linux |
| `-4`, `-6`, `-f inet\|inet6` | Supported | `AF_INET`/`AF_INET6` tables |
| `-p` | Supported | PID join with `process::list`; `users:(("name",pid=N,fd=-))`. No fd exists on Windows, so print `fd=-` or omit it. |
| `-H`, `-O`, `-Q` (no header, one line, no queues) | Supported | Output formatting |
| `-s` | Supported | `GetTcpStatisticsEx`/`GetUdpStatisticsEx` plus table counts; the layout will differ from Linux's `/proc/net/sockstat` fields |
| `state FILTER` (`established`, `listening`, `connected`, `synchronized`, `bucket`, `big`, individual states) | Supported | State enum |
| Address/port expressions (`sport`, `dport`, `src`, `dst`, `=`, `!=`, `<`, `>`, `and`, `or`, `not`) | Supported (subset) | Evaluated in Cash; no BPF |
| `Recv-Q`/`Send-Q` columns | Printed as `0` or `-` | Not exposed by IP Helper; document this |
| `-i` | Supported only when EStats is readable, otherwise refused | `GetPerTcpConnectionEStats` (RTT, cwnd, bytes). Enabling collection needs admin; do not enable it silently |
| `-e`, `-m`, `-o` (uid/inode, socket memory, timers) | Refused | No Windows equivalent |
| `-x` (Unix sockets) | Refused | Windows AF_UNIX sockets cannot be enumerated |
| `-w`, `-0`, `-d`, `-S`, `--vsock`, `--tipc`, `-M` | Refused | No table (raw, packet, DCCP, SCTP, vsock, TIPC, MPTCP) |
| `-K` (kill) | Refused, or later admin-only IPv4 | `SetTcpEntry(DELETE_TCB)`; needs admin and works for IPv4 only |
| `-Z`, `-z` (SELinux), `-N` (netns), `-b` (BPF), `-E` (events) | Refused | Not applicable |
| `-D FILE`, `-F FILE` | `-F` supported, `-D` refused | `-F` reads the filter from a file |
| `-j` (JSON, newer iproute2) | Optional | Useful for scripting |

`fuser -n tcp PORT` and `lsof -i [46][proto][@host][:port]` would use `sockets()`
directly. Only the file side of `fuser` and `lsof` needs Restart Manager
(`RmStartSession`/`RmRegisterResources`/`RmGetList`), which returns PIDs holding a file
without admin, but no file-descriptor numbers.

## Open questions

1. Should `-p` print a `service=` field for svchost PIDs, or keep Linux's output shape
   exactly?
2. Should `Recv-Q`/`Send-Q` print `0`, which scripts parse more easily, or `-`, which is
   more honest?
3. Should `ss` emit a one-time note when `netstat.exe` flags such as `-ano` are passed, to
   point Windows users at the Linux spellings?
4. Is `-i` without admin worth the complexity, given that EStats collection is usually
   off?
