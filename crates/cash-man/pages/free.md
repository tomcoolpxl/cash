---
name: free
summary: Show the machine's memory: used, free, cache and the page files, as procps' `free` does.
kind: Processes and network
see: top uptime
---

## Syntax

    free [OPTION]...

    free -h          memory and page files, each in the unit that suits it
    free -m -t       in mebibytes, with a total row
    free -s 2 -c 5   five times, two seconds apart

## Examples

The memory, readable, and only what is available:

<!-- parse only: the reader's own memory -->

    free -h
    free -m | awk '/^Mem:/ { print $7 " MiB available" }'
    free -s 2 -c 3

## How it works

`free` shows the machine's memory in procps' layout: `total`, `used`, `free`, `shared`,
`buff/cache` and `available` for `Mem:`, and the page files as `Swap:`. `-h` picks a unit
per number (`15Gi`, `628Mi`), `--si` counts in thousands, `-b`, `-k`, `-m`, `-g` fix the
unit, `-w` splits `buffers` from `cache`, `-t` adds a `Total:` row, `-v` a `Comm:` row,
`-L` puts the figures on one line, and `-s SECONDS` repeats (`-c COUNT` times).

## Options

    -h, --human      Each number in the unit that suits it.
    --si             Powers of 1000, not 1024.
    -b, -k, -m, -g   Bytes, kibibytes (the default), mebibytes, gibibytes.
    -w, --wide       Buffers and cache apart.
    -t, --total      A Total: row.
    -v, --committed  A Comm: row.
    -l, --lohi       Low: and High: rows.
    -L, --line       Everything on one line.
    -s SECONDS       Again every SECONDS.
    -c COUNT         COUNT times, with -s.

Exit status: 0.

## Different in cash

- `total` is the installed memory and `available` what Windows can hand out at once (free
  and standby pages), the same `MemAvailable` Linux reports; `used` is `total` less
  `available`, as procps counts it.
- `buff/cache` is Windows' system cache: the standby list, which holds file pages that
  can be dropped, plus the system's own working set. `free` is what is available beyond
  it, so `used + free + buff/cache` is `total`. Windows does not count `shared`, so it is 0.
- `Swap:` is the page files together, their sizes and the part in use; a machine without
  one shows zeros. `Comm:` is the commit limit and the commit charge, Task Manager's
  "Committed".
- `-l` prints Linux's `Low:` and `High:` rows for the sake of scripts; all memory is low.
- `-s` repeats until Ctrl-C.
