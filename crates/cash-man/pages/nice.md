---
names: nice renice
summary: Run a command at a lower (or higher) priority, or print the current niceness.
summary renice: Change the priority of running processes, by pid or by user.
kind: Processes and network
see: ps kill top
---

## Syntax

    nice [-n N] COMMAND [ARG]...
    renice [-n] PRIORITY [-p] PID...
    renice [-n] PRIORITY -u USER...

    nice COMMAND          at a lower priority: niceness 10
    nice -n 19 COMMAND    at the lowest
    nice -n -5 COMMAND    at a higher one
    nice                  the shell's own niceness
    renice -n 5 -p PID    a running process at niceness 5

## Examples

A long build that leaves the machine usable, and a process slowed after it started:

<!-- parse only: changes the priority of the reader's own processes -->

    nice -n 19 cargo build
    nice -n -5 ./bench
    renice -n 5 -p "$(pgrep -x node)"

## How it works

`nice COMMAND` runs a command at a lower priority; `-n N` adjusts the niceness by N (10
without `-n`), and a negative N raises the priority. `nice` alone prints the shell's
niceness. `renice PRIORITY -p PID...` changes the priority of running processes, and
`-u USER` of every process a user runs; `--relative N` adds to what a process has. Each
process is reported as `PID (process ID) old priority N, new priority M`.

## Options

nice:

    -n N          Add N to the niceness: 10 without.

renice:

    -n N          The new niceness.
    -p PID        The processes, by id: the default.
    -u USER       Every process USER runs.
    --relative N  Add N to what each has.

Exit status: nice gives the command's, 125 when nice itself fails, 127 when the
command is not found; renice 0, or 1 when a process could not be changed.

## Different in cash

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
  (`tom`, `PC\tom`), not a numeric id, and changes every one of the account's processes
  it may open, the shell, Explorer and your browser among them; the ones it may not,
  elevated or another user's, are reported as refused.
