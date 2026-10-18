---
name: builtin
summary: Run a builtin, skipping functions of the same name.
kind: Bash builtins
see: command type enable
---

## Syntax

    builtin NAME [ARG]...

    builtin cd DIR      cd itself, even inside a function called cd

## Examples

A function that wraps `cd`, as many profiles have:

    cd() { builtin cd "$@" && echo "now in ${PWD##*/}"; }
    mkdir -p projects
    cd projects

An `echo` of your own that still reaches the real one:

    echo() { builtin echo "[$*]"; }
    echo hello

A name that is not a builtin:

    builtin nosuch 2>/dev/null || echo "nosuch is not a builtin"

## How it works

`builtin NAME ARGS` runs the builtin NAME, skipping any function or alias of that name:
the way a function named after a builtin reaches it. `command` does the same and also
reaches programs; `builtin` reaches only builtins, and fails for anything else.

## Options

None: the first argument is the builtin, the rest are its own.

Exit status: the builtin's; 1 when NAME is not a builtin.

## Different in cash

- cash carries its tools as builtins, so `builtin ls` and `builtin sort` run them,
  where Bash says they are not builtins.
