---
name: false
summary: Do nothing, unsuccessfully.
kind: Bash builtins
see: true test
---

## Syntax

    false [ARG]...

    false              fail, doing nothing
    if false; then     a branch that never runs

## Examples

The status it leaves:

    false
    echo "status $?"

The other branch, on purpose:

    if false; then echo never; else echo "the else branch"; fi
    false || echo "after a failure"

## How it works

`false` does nothing and exits 1; it ignores its arguments. It stands for a command
that fails: a condition that is never met, or a way to see what a script does when a
command fails.

## Options

None: every argument is ignored.

Exit status: 1.
