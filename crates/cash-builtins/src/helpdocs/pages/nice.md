---
names: nice renice
see: ps kill top
---
## Description

- `nice COMMAND` runs a command at a lower priority; `-n N` adjusts the niceness by N
  (10 without `-n`), a negative N raises priority. `nice` alone prints the shell's
  niceness.
- `renice PRIORITY -p PID...` changes the priority of running processes, and `-u USER`
  of every process a user runs; `--relative N` adds to what a process has. Each process
  is reported as `PID (process ID) old priority N, new priority M`.

## Windows notes

- Windows schedules by six priority classes, not forty nice values, so a niceness picks a
  class: -20..-11 high, -10..-1 above normal, 0 normal, 1..10 below normal, 11..19 idle.
  A class reads back as the middle of its range: `nice -n 10 nice` prints 5, `renice -n
  7 PID` reports `new priority 5`, and `ps` shows the class as a base priority.
- Raising priority needs no privilege, so `nice -n -5` and `renice -5` work for your own
  processes. Realtime is never set. Another user's processes, elevated ones and Windows'
  own are refused with `Permission denied`; an elevated shell reaches more of them.
- `nice` creates the program in the class; a program it starts in turn inherits the
  class only when it is idle or below normal, as Windows has it. A builtin named by
  `nice` (`nice -n 10 cat`) runs inside the shell, at the shell's own priority.
- `renice -g` is refused: Windows has no process groups. `-u` takes an account name
  (`tom`, `PC\tom`), not a numeric id, and reports each of the account's processes;
  the ones you may not open, elevated or another user's, are reported as refused.

## Examples

```
nice -n 19 cargo build
nice -n -5 ./bench
renice -n 5 -p $(pgrep -x node)
renice --relative -10 1234
```
