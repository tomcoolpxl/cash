---
see: ps pstree kill uptime
---
## Description

`top` shows the processes using the machine, refreshed every 3 seconds, with CPU and
memory meters and a load average: Windows has no `top`, and procps was never ported.
`-b` prints plain text for a pipe or a log, `-n COUNT` stops after COUNT refreshes.

## Keys

- `P`, `M`, `T`, `N`: sort by CPU, memory, time or pid.
- `?` or `h`: the keys. Space or Enter: refresh now.
- `V`: show processes as a tree. `1`: a meter per processor.
- `o` or `/`: show only names containing some text; `=` shows all again.
- `d`: change the interval. `k`: send a signal, by `kill`'s rules.
- `t`, `m`: meters, text or nothing for CPU and memory. `z`: colour on or off.
- `q`: quit, and the screen is put back.

## Windows notes

The load average is Linux's definition, processors busy plus threads ready to run,
counted from Windows' process snapshot and smoothed from when `top` starts. Uptime counts
from the last full boot, as Task Manager does: with Fast Startup, turning the machine on
resumes the count.
