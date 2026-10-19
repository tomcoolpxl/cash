---
name: ss
summary: Show sockets, as iproute2's `ss` does.
kind: Processes and network
see: fuser ping
---

## Syntax

    ss [OPTION]... [FILTER]

    ss -tulpn             what is listening, TCP and UDP, with the process
    ss -tn state established    the open TCP connections
    ss -s                 counts of each kind

## Examples

What listens, a connection to a port, and the counts:

<!-- parse only: lists the reader's own sockets -->

    ss -tulpn
    ss -tn state established '( dport = :443 )'
    ss -ltn | grep -q ':5432 ' && echo "postgres is listening"
    ss -s

## How it works

`ss` shows sockets in iproute2's layout, so Linux habits work: `ss -tulpn` for what is
listening, `ss -ltn | grep -q ':5432 '` in a script. State filters and the
`sport`/`dport`/`src`/`dst`/`dev` expressions are supported, and options are spelled and
combined as in iproute2 7.2 (`--num` for `--numeric`, `-A 'all,!udp'`).

## Options

    -t, -u         TCP, UDP.
    -l             Listening sockets only.
    -a             Listening and the rest.
    -n             Numbers, not service names.
    -p             The process of each.
    -4, -6         IPv4 or IPv6 only.
    -s             Counts of each kind.
    -r             Host names for addresses.
    -i             Each connection's statistics.
    -K             Close the connections matched; elevated.
    -B             Sockets bound but not connected.
    -A QUERY       The socket tables by name: all, tcp, udp, ...
    state STATE    Only sockets in STATE: established, listening, ...
    EXPRESSION     sport, dport, src, dst, dev, with and, or, not.

Exit status: 0, or 1 for an option or a filter it does not take.

## Different in cash

It reads Windows' TCP and UDP tables, IPv4 and IPv6. `-p` prints `fd=-`, and a service
inside `svchost.exe` is named (`service=NAME`); UDP sockets have no peer and are always
`UNCONN`. Windows collects a connection's statistics only once an elevated shell switches
them on: `ss -i` shows the SYN's MSS values, and `sudo ss -i` switches collection on (it
stays on until the connection closes) and shows round-trip times, windows and counts;
until then Recv-Q and Send-Q print `0`. `dev NAME` matches an IPv6 socket's scope; other
sockets have no device. `-K` closes IPv4 connections only, and only from an elevated
shell (`sudo ss -K ...`). `-B` lists what `netstat -q` calls `BOUND`. Options with nothing
behind them on Windows (`-x`, `-e`, `-m`, `-o`) are refused, and `ss -ano`, a `netstat`
habit, gets a hint. `netstat.exe` is untouched.
