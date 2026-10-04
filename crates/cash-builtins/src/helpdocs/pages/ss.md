---
see: fuser ping
spec: D51
---
## Description

`ss` shows sockets in iproute2's layout, so Linux habits work: `ss -tulpn` for what is
listening, `ss -ltn | grep -q ':5432 '` in a script. State filters and the
`sport`/`dport`/`src`/`dst` expressions are supported, and options are spelled and
combined as in iproute2 7.2 (`--num` for `--numeric`, `-A 'all,!udp'`).

## Windows notes

It reads Windows' TCP and UDP tables, IPv4 and IPv6. Windows keeps no queue sizes, so
Recv-Q and Send-Q print `0`; `-p` prints `fd=-`, and a service inside `svchost.exe` is
named (`service=NAME`); UDP sockets have no peer and are always `UNCONN`. `-K` closes
IPv4 connections only, and only from an elevated shell (`sudo ss -K ...`). `-B` lists
what `netstat -q` calls `BOUND`. Options with nothing behind them on Windows (`-x`, `-e`,
`-m`, `-o`, `-i`) are refused, and `ss -ano`, a `netstat` habit, gets a hint.
`netstat.exe` is untouched.

## Examples

```
ss -tulpn
ss -tn state established '( dport = :443 )'
ss -tnr dst 192.168.1.0/24
```
