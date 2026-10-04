---
see: ss
spec: D57
---
## Description

`ping HOST` sends ICMP echo requests with Linux's (iputils') options and output: `-c
COUNT`, `-i INTERVAL`, `-W TIMEOUT`, `-w DEADLINE`, `-s SIZE`, `-4`, `-6`, `-q`. It exits
0 when a reply came, 1 when none did, 2 on error, so `ping -c 1 host && ...` works.

## Windows notes

Windows' `ping.exe` reads `-c` as a routing compartment and, unelevated, refuses: every
host would look down. Inside cash, `ping` is this one; `ping.exe` by that name or by its
path is Windows'. `ping -n 3 host`, the Windows habit, is refused with a hint that
`-c 3` is meant.

- One echo is in flight at a time: a reply slower than the interval counts as lost.
- IPv6 replies show no `ttl=`; `-s` stops at 65500 bytes.
- Options that need a raw socket (`-f`, `-l`, `-p`, `-R`, `-T`, `-I`) are refused.

## Examples

```
ping -c 3 github.com
until ping -c 1 -W 1 db.local >/dev/null; do sleep 1; done
```
