---
name: enable
summary: Turn builtins on or off, or list them.
kind: Bash builtins
see: builtin command type help
---

## Syntax

    enable [-anps] [NAME...]

    enable -n sort    turn the builtin sort off: the program on PATH runs
    enable sort       turn it on again
    enable -a         every builtin, on or off
    enable -s         the special builtins

## Examples

The program on `PATH` in place of a builtin, and back:

    enable -n echo
    type -t echo
    enable echo
    type -t echo

A name that is not a builtin:

    enable nosuch 2>/dev/null || echo "nosuch is not a builtin"

## How it works

`enable -n NAME` turns the builtin NAME off, so the name runs a function or the program
`PATH` finds, as if the builtin were not there; `enable NAME` turns it on again. With no
NAME, `enable` lists the builtins that are on, as `enable` commands; `-n` lists the ones
that are off, `-a` all of them, each with `-n` when off, and `-s` only the special
builtins, the ones POSIX treats apart (`export`, `set`, `trap`, ...).

## Options

    -a    List every builtin, on or off.
    -n    Turn each NAME off; with no NAME, list the ones that are off.
    -p    List as enable commands: the default.
    -s    Only the special builtins.

Exit status: 0; 1 when a NAME is not a builtin; 2 for `-f` or `-d`.

## Different in cash

- cash carries its tools as builtins, so `enable -n sort` or `enable -n ls` is how to
  run the `sort.exe` or `ls.exe` on `PATH` instead, Git's for one.
- `enable -f FILE` and `enable -d`, which load and remove builtins from a shared
  library, are refused: cash cannot load Bash's loadable builtins.
