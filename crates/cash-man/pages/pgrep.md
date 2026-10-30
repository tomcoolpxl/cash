---
names: pgrep pkill pidof killall
summary: Find processes by name and other attributes.
summary pkill: Signal processes by name and other attributes.
summary pidof: Print the process ids of programs by name.
summary killall: Signal every process with a given name.
kind: Processes and network
see: kill ps job-control
---

## Syntax

    pgrep [OPTION]... PATTERN
    pkill [-SIGNAL] [OPTION]... PATTERN
    pidof NAME...
    killall [-SIGNAL] [-w] NAME...

    pgrep -l node       the ids and names of node's processes
    pgrep -x code       only processes named code exactly
    pkill -x node       ask every node process to end
    pidof notepad       the ids of notepad's processes
    killall -w code     end every code process, and wait until they have

## Examples

Find, then end:

<!-- parse only: acts on the reader's own processes -->

    pgrep -l python
    pgrep -P $$
    pkill -x node
    killall -w code
    pidof notepad

## How it works

`pgrep PATTERN` prints the ids of the processes whose program name matches the regular
expression PATTERN; `-x` matches the whole name, `-l` adds the names, `-P PID` takes
only the children of PID, `-n` the newest and `-o` the oldest, `-c` counts them, and
`-d STRING` puts STRING between the ids. `pkill` takes the same options and sends the
processes a signal, `TERM` unless you name another (`pkill -KILL -x node`). `pidof
NAME` prints the ids of the processes running a program, highest first. `killall NAME`
signals every process running a program; `-w` waits for them to end.

## Options

    -x          The whole name must match.
    -l          With the names (pgrep).
    -P PID      Only the children of PID.
    -n          Only the newest.
    -o          Only the oldest.
    -c          The count, not the ids (pgrep).
    -d STRING   STRING between the ids: a newline without (pgrep).
    -SIGNAL     The signal to send: TERM without (pkill, killall).
    -w          Wait until they have ended (killall).

Exit status: 0 when a process was found; 1 when none was; 2 for a bad option.

## Different in cash

- Names match in any case, and `.exe` is optional on either side: `pidof notepad`,
  `pidof NOTEPAD.EXE` and `pgrep -x notepad` all find `notepad.exe`.
- Signals go as `kill` sends them: `TERM` asks a program with a window to close and
  ends it after five seconds, gives a background job a Ctrl-Break and a second, ends
  any other console program at once; `KILL` ends at once; `STOP` and `CONT` suspend
  and resume.
- They never signal the shell running them, Windows' own system processes (`csrss`,
  `lsass`, `winlogon`, ...) or service accounts' processes. A process you may not open
  is skipped, not reported as a failure.
- Not supported: `-f` (another process's command line), and the user, group, session
  and terminal selectors.
