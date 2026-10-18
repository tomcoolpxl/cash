---
name: trap
summary: Run a command when the shell receives a signal or event.
kind: Bash builtins
see: kill job-control
draft: yes
---

## How it works

`trap COMMAND SIGNAL...` runs a command when the shell receives a signal or meets an
event: `EXIT`, `ERR`, `DEBUG`, `RETURN`, `INT`, `TERM`, `CHLD`, ... `trap -p` lists the
traps, `trap - SIGNAL` removes one.

## Different in cash

Windows delivers no Unix signals, so cash raises the events itself:

- `INT`: Ctrl-C, after the foreground program has ended, or at once between commands. Without a trap a script ends with 130, its `EXIT` trap run.
- `EXIT` runs to its end; a second Ctrl-C while it runs, two seconds or more after the
  first, forces the shell down.
- `TERM`, `HUP` and the rest: when `kill` sends them to the shell itself (`kill -TERM
  $$`), the shell runs the trap, as Bash does. Another process cannot send cash a
  signal Windows does not have: a cash script another process sends one ends at once
  with 128 plus its number, its traps, `EXIT`'s too, not run.
- `CHLD`: once for each child process cash starts and reaps, and for each background job. Builtins and bundled tools run inside cash and raise none.
