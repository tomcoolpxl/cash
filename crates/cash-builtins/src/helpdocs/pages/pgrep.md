---
names: pgrep pkill pidof killall
see: kill ps job-control
spec: D54 D21 D22 D19
---
## Description

- `pgrep PATTERN` prints the ids of the processes whose program name matches; `-P PID`
  selects children of a process, `-l` adds names, `-x` matches the whole name.
- `pkill PATTERN` sends them a signal, `TERM` unless you name another.
- `pidof NAME` prints the ids of the processes running a program, highest first.
- `killall NAME` signals every process running a program; `-w` waits for them to end.

## Windows notes

- Names match in any case, and `.exe` is optional on either side: `pidof notepad`,
  `pidof NOTEPAD.EXE` and `pgrep -x notepad` all find `notepad.exe`.
- Signals go as `kill` sends them: `TERM` asks a program with a window to close and
  ends it after five seconds, ends a console program at once; `KILL` ends at once;
  `STOP` and `CONT` suspend and resume.
- They never signal the shell running them, Windows' own system processes (`csrss`,
  `lsass`, `winlogon`, ...) or service accounts' processes. A process you may not open
  is skipped, not reported as a failure.
- Not supported: `-f` (another process's command line), and the user, group, session
  and terminal selectors.

## Examples

```
pkill -x node
killall -w code
pgrep -l python
```
