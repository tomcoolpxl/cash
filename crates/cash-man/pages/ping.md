---
name: ping
summary: Send ICMP echo requests to a host, with Linux's options.
kind: Processes and network
see: ss nc
---

## Syntax

    ping [OPTION]... HOST

    ping -c 3 HOST       three requests, then the summary
    ping -c 1 -W 1 HOST  one, waiting a second for it
    ping -q -c 5 HOST    only the summary

## Examples

Three requests, and a wait until a host answers:

<!-- parse only: sends packets to hosts of the reader's own -->

    ping -c 3 github.com
    until ping -c 1 -W 1 db.local >/dev/null; do sleep 1; done

## How it works

`ping HOST` sends ICMP echo requests with Linux's (iputils') options and output: `-c
COUNT`, `-i INTERVAL`, `-W TIMEOUT`, `-w DEADLINE`, `-s SIZE`, `-4`, `-6`, `-q`. It exits
0 when a reply came, 1 when none did, 2 on error, so `ping -c 1 host && ...` works.

## Options

    -c COUNT       Stop after COUNT replies or losses.
    -i SECONDS     Wait SECONDS between requests: 1 without.
    -W SECONDS     Wait SECONDS for each reply.
    -w SECONDS     Stop after SECONDS, however many were sent.
    -s SIZE        SIZE bytes of data in each: 56 without.
    -q             Only the summary.
    -4, -6         IPv4 or IPv6.
    -n             Numbers: no names for the replies.

Exit status: 0 when a reply came; 1 when none did; 2 for an error.

## Different in cash

Windows' `ping.exe` reads `-c` as a routing compartment and, unelevated, refuses: every
host would look down. Inside cash, `ping` is this one; `ping.exe` by that name or by its
path is Windows'. `ping -n 3 host`, the Windows habit, is refused with a hint that
`-c 3` is meant.

- One echo is in flight at a time: a reply slower than the interval counts as lost.
- IPv6 replies show no `ttl=`; `-s` stops at 65500 bytes.
- Options that need a raw socket (`-f`, `-l`, `-p`, `-R`, `-T`, `-I`) are refused.
