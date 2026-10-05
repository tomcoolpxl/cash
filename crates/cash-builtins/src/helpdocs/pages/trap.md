---
see: kill job-control
---
## Description

`trap COMMAND SIGNAL...` runs a command when the shell receives a signal or meets an
event: `EXIT`, `ERR`, `DEBUG`, `RETURN`, `INT`, `TERM`, `CHLD`, ... `trap -p` lists the
traps, `trap - SIGNAL` removes one.

## Windows notes

Windows delivers no Unix signals, so cash raises the events itself:

- `INT`: Ctrl-C, after the foreground program has ended, or at once between commands. Without a trap a script ends with 130, its `EXIT` trap run.
- `EXIT` runs to its end; a second Ctrl-C while it runs forces the shell down.
- `TERM`, `HUP` and the rest: when `kill` sends them to the shell itself (`kill -TERM
  $$`), the shell runs the trap, as Bash does. Another process cannot send cash a
  signal Windows does not have.
- `CHLD`: once for each child process cash starts and reaps, and for each background job. Builtins and bundled tools run inside cash and raise none.
