---
title: Jobs, Ctrl-C and Ctrl-Z on Windows
summary: Job objects, what Ctrl-C and Ctrl-Z do, `kill` and its signals, and `detach`.
see: jobs fg bg kill wait disown detach trap cashctl elevation
spec: D6 D11 D13 D14 D19 D21 D22 D42 D45 D70
---
## Every job is a job object

Windows has no process groups or signals as Unix has them, so cash runs every pipeline
and background job in a Windows job object of its own, inside one job for the whole
session (D6). Killing a job ends its whole tree, grandchildren included, and nothing
cash starts can outlive cash being killed, even from Task Manager.

Exceptions, by design:

- a command that has finished is released, not reaped: what it left running keeps
  running, as in Bash (`code .` starts VS Code and exits);
- GUI programs outlive an orderly exit of cash, as they do PowerShell's;
  `cashctl gui-apps close` makes them close with cash for this session;
- `detach COMMAND` starts something meant to outlive the shell (D45);
- elevated programs cannot be put in cash's job, so they escape it (D42; see
  `help elevation`).

## Ctrl-C

A foreground program gets Ctrl-C directly, as often as you press it, so a REPL keeps
its session. A script whose program died of Ctrl-C ends with status 130, its `EXIT`
trap run; a `trap ... INT` runs instead if you set one (D13, D14). At the prompt, Ctrl-C
abandons the line.

Background jobs started with `&` at the prompt lead a console group of their own, so the
keyboard's Ctrl-C passes them by. Under `fg`, cash relays Ctrl-C to such a job as a
Ctrl-Break, and a second Ctrl-C ends the job's tree.

## Ctrl-Z

Windows has no Ctrl-Z signal: it is a key, which programs that read the keyboard use as
end of input. While the interactive shell waits for a foreground job, it takes a Ctrl-Z
nobody read for 200 ms, suspends the job's processes, and prints `[1]+ Stopped` (D19).
`fg`, `bg` and `kill -CONT %1` resume it. A program reading the keyboard takes the key
itself. At the prompt, Ctrl-Z is the line editor's undo.

## kill

- `kill %1` reaches the job's whole tree; `kill 1234` that one process (D22).
- `kill` and `kill -TERM` ask first: a program with a window gets `WM_CLOSE`, as from its
  close button, and five seconds before it is terminated; a console program is ended at
  once, unless it is a background job, which gets a Ctrl-Break (D21).
- `kill -9` terminates at once. A killed program's status is 128 plus the signal number:
  143 for `TERM`, 137 for `KILL`.
- `kill -STOP` and `kill -CONT` suspend and resume, as Ctrl-Z and `fg` do.
- A pid from `$!` keeps naming that job after it ended, so `kill "$pid"` in a cleanup
  trap cannot hit a program Windows gave the number to since.

## Background jobs inside the shell

cash does not fork: a background job, a subshell or a `$(...)` that runs shell code runs
on a thread of cash's own process (D70). `$!` still names it, with a number no Windows
process can have, and `kill` and `wait` take it. At most 256 run at once, or
`CASH_MAX_SUBSHELLS`.
