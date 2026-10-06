---
see: top uptime
---
## Description

`free` shows the machine's memory in procps' layout: `total`, `used`, `free`, `shared`,
`buff/cache` and `available` for `Mem:`, and the page files as `Swap:`. `-h` picks a unit
per number (`15Gi`, `628Mi`), `--si` counts in thousands, `-b`, `-k`, `-m`, `-g` fix the
unit, `-w` splits `buffers` from `cache`, `-t` adds a `Total:` row, `-v` a `Comm:` row,
`-L` puts the figures on one line, and `-s SECONDS` repeats (`-c COUNT` times).

## Windows notes

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

## Examples

```
free -h
free -m -t
free -s 2 -c 5
```
