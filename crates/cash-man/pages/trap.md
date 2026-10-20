---
name: trap
summary: Run commands when the shell gets a signal or meets an event.
kind: Bash builtins
see: kill set job-control
---

## Syntax

    trap [-lp] [[COMMAND] SIGNAL...]

    trap 'CMD' EXIT     run CMD when the shell exits
    trap 'CMD' ERR      run CMD when a command fails
    trap 'CMD' INT      run CMD on Ctrl-C, instead of ending
    trap - INT          take the trap away again
    trap '' INT         ignore Ctrl-C
    trap -p             list the traps

## Examples

Clean up when a script ends, however it ends:

    tmp=$(mktemp)
    trap 'rm -f "$tmp"; echo cleaned' EXIT
    echo working

Say where a command failed:

    trap 'echo "failed with $? on line $LINENO"' ERR
    false
    echo 'went on'

A trap for the end of a function, and the list of traps:

    trap 'echo bye' EXIT
    trap -p EXIT

Take a trap away:

    trap 'echo never' EXIT
    trap - EXIT
    echo done

## How it works

`trap COMMAND SIGNAL` makes the shell run COMMAND, a string read as shell code when it
runs, each time SIGNAL comes. Besides real signals (`INT`, `TERM`, `HUP`, ...) there
are events of the shell's own: `EXIT` when it exits, `ERR` when a command fails (as
`set -e` would see it), `DEBUG` before each command, `RETURN` when a function or a
sourced file returns.

`DEBUG` runs where Bash runs it: before each simple command, each stage of a pipeline
included, `[[ ]]` and `(( ))`, each turn of a `for` loop and each part of a `for ((…))`,
and once before a `case` or a `select`, with `$BASH_COMMAND` the command about to run and
`$LINENO` its line. A function, and a subshell, runs it only under `set -T` (or for a
function marked `declare -ft`), and then first at the function's start. Under
`shopt -s extdebug`, a `DEBUG` trap that fails skips the command, and one that ends with
2 in a function returns from it.

A trap on `INT` keeps Ctrl-C from ending a script: the trap runs and the script goes
on. `trap '' SIGNAL` ignores the signal; `trap - SIGNAL` puts back what it did before
any trap. Quote COMMAND in single quotes, so its variables are read when it runs, not
when the trap is set.

## Options

    -p [SIG]   Write the traps as commands that set them again.
    -l         List the signals' names and numbers.

Exit status: 0, or 1 for a signal that is not one.

## Different in cash

Windows delivers no Unix signals, so cash raises the events itself:

- `INT`: Ctrl-C, after the foreground program has ended, or at once between commands.
  Without a trap a script ends with 130, its `EXIT` trap run.
- `EXIT` runs to its end; a second Ctrl-C while it runs, two seconds or more after the
  first, forces the shell down.
- `TERM`, `HUP` and the rest: when `kill` sends them to the shell itself (`kill -TERM
  $$`), the shell runs the trap, as Bash does. Another process cannot send cash a
  signal Windows does not have: a cash script another process sends one ends at once
  with 128 plus its number, its traps, `EXIT`'s too, not run.
- `CHLD`: once for each child process cash starts and reaps, and for each background
  job. Builtins and bundled tools run inside cash and raise none.
