---
name: top
summary: Show the processes using the machine, updated live.
kind: Processes and network
see: ps pstree kill uptime
---

## Syntax

    top [OPTION]...

    top              live, full screen, until q
    top -b -n 1      once, as text, for a pipe or a log
    top -o mem       sorted by memory

## Examples

Live, once to a file, and the five that use most memory:

<!-- parse only: lists the reader's own processes -->

    top
    top -b -n 1 > snapshot.txt
    top -b -n 1 -o mem | head -12

## How it works

`top` shows the processes using the machine, refreshed every 3 seconds, with CPU and
memory meters and a load average: Windows has no `top`, and procps was never ported.
`-b` prints plain text for a pipe or a log, `-n COUNT` stops after COUNT refreshes.

### Keys

- `P`, `M`, `T`, `N`: sort by CPU, memory, time or pid.
- `?` or `h`: the keys. Space or Enter: refresh now.
- `V`: show processes as a tree. `1`: a meter per processor.
- `o` or `/`: show only names containing some text; `=` shows all again.
- `d`: change the interval. `k`: send a signal, by `kill`'s rules.
- `t`, `m`: meters, text or nothing for CPU and memory. `z`: colour on or off.
- `q`: quit, and the screen is put back.

## Options

    -b             Plain text, not a live screen.
    -n COUNT       Stop after COUNT updates.
    -d SECONDS     Update every SECONDS: 3 without.
    -o FIELD       Sort by FIELD: cpu (the default), mem, time, pid.
    -u USER        Only USER's processes.

Exit status: 0.

## Different in cash

The load average is Linux's definition, processors busy plus threads ready to run,
counted from Windows' process snapshot and smoothed from when `top` starts. Uptime counts
from the last full boot, as Task Manager does: with Fast Startup, turning the machine on
resumes the count.
