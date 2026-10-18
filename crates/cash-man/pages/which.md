---
name: which
summary: Say what a command name runs.
kind: cash's shell commands
see: type command hash
---

## Syntax

    which [-ap] NAME...

    which NAME       what cash runs for NAME
    which -a NAME    every match, in the order cash looks
    which -p NAME    the program on PATH alone

## Examples

What a name runs:

    which cd
    which ls

Every match, the builtin first:

    which -a sort

Use what `which` prints to run the command:

    LS=$(which ls)
    "$LS" -d .

Whether a program is there at all:

    which nosuchprogram >/dev/null 2>&1 || echo 'not installed'

## How it works

`which NAME` says what cash would run for the name: an alias, a function, a builtin or
a program on `PATH`, in the order cash looks for them. `-a` lists every match, and
`-p` searches `PATH` only. It returns 1 when a NAME finds nothing, so it answers
"is this installed?" in a script.

`type` says the same in Bash's words, and `command -v` in POSIX's.

## Options

    -a    Every match, not only the first.
    -p    Programs on PATH only.

Exit status: 0 when every NAME was found; 1 otherwise.

## Different in cash

- Many of cash's builtins are programs elsewhere (`ls`, `sed`, `ps`). For those, `which`
  prints a path that cash can run, cash's own executable with the name appended:
  `C:/.../cash.exe/ls`. No file is there, but a script that does `LS=$(which ls);
  "$LS" -la` works, as do `exec`, `xargs` and `find -exec` with it. A program outside
  cash cannot run that path. Bash's own builtins, which are no program anywhere, say
  `cd: shell builtin`.
- A program found on `PATH` is written `C:/...`, as cash writes paths.
