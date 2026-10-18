---
name: which
summary: Say what cash would run for a name, as a runnable path.
kind: cash's shell commands
see: type where command hash
draft: yes
---

## Examples

    which -a sort         # the builtin, then each sort.exe on PATH
    which -p python

## How it works

`which NAME` says what cash would run for the name: an alias, a function, a builtin or
a program on `PATH`, in the order cash looks for them. `-a` lists every match, and
`-p` searches `PATH` only.

## Different in cash

Many of cash's builtins are programs elsewhere (`ls`, `sed`, `ps`). For those, `which`
prints a path that cash can run, cash's own executable with the name appended:
`C:/.../cash.exe/ls`. No file is there, but a script that does `LS=$(which ls); "$LS"
-la` works, as do `exec`, `xargs` and `find -exec` with it. A program outside cash
cannot run that path. Bash's own builtins, which are no program anywhere, still say
`cd: shell builtin`.
