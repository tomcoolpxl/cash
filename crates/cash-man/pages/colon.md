---
name: :
summary: Do nothing, successfully; its arguments are still expanded.
kind: Bash builtins
see: true
---

## Syntax

    : [ARG]...

    :                     succeed, doing nothing
    : "${VAR:=default}"   give VAR a value if it has none
    : > FILE              empty FILE, or make it

## Examples

A default for a variable, given once:

    : "${name:=world}"
    echo "hello $name"

A file emptied:

    printf 'old\n' > out.txt
    : > out.txt
    wc -c < out.txt

Counting in an expansion:

    n=5
    : $((n += 1))
    echo "$n"

## How it works

`:` does nothing and exits 0, as `true` does, but its arguments are expanded and its
redirections made: so `: "${VAR:=value}"` gives VAR a default through the expansion,
`: > file` empties a file, and `: $((n += 1))` counts. `while :` is a loop that runs
until a `break`.

## Options

None: every argument is expanded, then ignored.

Exit status: 0.
