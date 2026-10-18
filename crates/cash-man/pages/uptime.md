---
name: uptime
summary: Show how long the system has been running.
kind: Bundled coreutils, from uutils
see: free ps
---

## Syntax

    uptime [OPTION]

    uptime       the time, how long since the machine started, and the users
    uptime -p    only how long, in words
    uptime -s    when it started

## Examples

How long, and since when:

<!-- parse only: the reader's own machine -->

    uptime
    uptime -p
    uptime -s

## How it works

`uptime` prints the time now, how long the machine has been running since it started,
and how many users are logged in. `-p` prints only how long, in words (`up 3 days, 2
hours`), and `-s` when it started, as `YYYY-MM-DD HH:MM:SS`.

## Options

    -p, --pretty    How long, in words.
    -s, --since     When the machine started.

Exit status: 0.

## Different in cash

- Windows keeps no load average, so `uptime` leaves it out.
